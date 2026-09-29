//! Orchestration loop: the LLM ↔ vault bridge.
//!
//! This is *not* an MCP client connection. The tool layer is shared in-process
//! with the MCP server ([`ObsidianServer::dispatch`]), so the loop works without
//! a second process, pipes or a JSON-RPC round trip. `serve` still exposes the
//! very same tools to external MCP hosts over stdio.

use std::io::Write;
use std::sync::Arc;

use anyhow::{Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::mcp_server::{ObsidianServer, SERVER_INSTRUCTIONS};
use crate::ollama_client::{ChatMessage, OllamaClient, ToolCall};

/// Hard cap on tool-call rounds so a confused model cannot loop forever.
pub const DEFAULT_MAX_ROUNDS: usize = 6;

/// What a single prompt produced.
#[derive(Debug, Clone)]
pub struct AskOutcome {
    pub answer: String,
    /// Model rounds used (1 = no tool calls were made).
    pub rounds: usize,
    /// Number of tool calls executed.
    pub tool_calls: usize,
    /// `(tool name, arguments)` pairs, in execution order.
    pub trace: Vec<(String, String)>,
}

/// Runs prompts against Ollama, executing vault tools on demand.
pub struct Host {
    client: OllamaClient,
    server: Arc<ObsidianServer>,
    use_tools: bool,
    max_rounds: usize,
}

impl Host {
    pub fn new(client: OllamaClient, server: Arc<ObsidianServer>) -> Self {
        Self {
            client,
            server,
            use_tools: true,
            max_rounds: DEFAULT_MAX_ROUNDS,
        }
    }

    /// Disable tool calling (plain chat round trip, used for connectivity tests).
    pub fn without_tools(mut self) -> Self {
        self.use_tools = false;
        self
    }

    /// Override the maximum number of model rounds per prompt.
    pub fn with_max_rounds(mut self, max_rounds: usize) -> Self {
        self.max_rounds = max_rounds.max(1);
        self
    }

    /// Send one prompt, execute any requested tools and return the final answer.
    pub async fn ask(&self, prompt: &str) -> Result<AskOutcome> {
        let tools = if self.use_tools {
            ObsidianServer::tool_specs()
        } else {
            Vec::new()
        };
        let mut messages = vec![
            ChatMessage::system(self.system_prompt()),
            ChatMessage::user(prompt),
        ];
        let mut trace = Vec::new();

        for round in 1..=self.max_rounds {
            let turn = self.client.chat(&messages, &tools).await?;
            if !turn.has_tool_calls() {
                let answer = if turn.content.trim().is_empty() {
                    "(the model returned an empty response)".to_string()
                } else {
                    turn.content.clone()
                };
                return Ok(AskOutcome {
                    answer,
                    rounds: round,
                    tool_calls: trace.len(),
                    trace,
                });
            }

            tracing::info!(
                round,
                calls = turn.tool_calls.len(),
                "model requested tool calls"
            );
            messages.push(ChatMessage::assistant_tool_calls(
                Some(turn.content.clone()),
                &turn.tool_calls,
            ));
            for call in &turn.tool_calls {
                let result = self.execute(call);
                trace.push((call.name.clone(), compact(&call.arguments)));
                messages.push(ChatMessage::tool_result(call, result));
            }
        }

        bail!(
            "model kept requesting tools after {} rounds; raise --max-rounds or narrow the question",
            self.max_rounds
        )
    }

    /// Execute one tool call, converting failures into model-readable text.
    fn execute(&self, call: &ToolCall) -> String {
        tracing::info!(tool = %call.name, arguments = %compact(&call.arguments), "executing tool");
        match self.server.dispatch(&call.name, &call.arguments) {
            Ok(text) => text,
            Err(error) => format!("ERROR: {error}"),
        }
    }

    /// System prompt: identical guidance to the MCP `instructions` field.
    fn system_prompt(&self) -> String {
        format!(
            "{SERVER_INSTRUCTIONS}\n\nThe vault contains {} note(s). Answer from the returned \
             notes only; if nothing matches, say so instead of inventing content.",
            self.server.vault().notes().len()
        )
    }

    /// Interactive read-eval-print loop over stdin/stdout.
    pub async fn chat_loop(&self) -> Result<()> {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mode = if self.use_tools {
            "tools on"
        } else {
            "tools off"
        };
        println!(
            "Chatting with {} at {} ({mode}). Type /exit to quit.",
            self.client.model(),
            self.client.chat_url()
        );

        loop {
            print!("you> ");
            std::io::stdout().flush().ok();
            let Some(line) = lines.next_line().await? else {
                println!();
                return Ok(());
            };
            let prompt = line.trim();
            if prompt.is_empty() {
                continue;
            }
            if matches!(prompt, "/exit" | "/quit" | "exit" | "quit") {
                return Ok(());
            }

            match self.ask(prompt).await {
                Ok(outcome) => {
                    for (tool, arguments) in &outcome.trace {
                        println!("  · {tool} {arguments}");
                    }
                    println!("llm> {}\n", outcome.answer.trim());
                }
                Err(error) => {
                    eprintln!("error: {error:#}");
                }
            }
        }
    }
}

/// Single-line rendering of tool arguments for logs and traces.
fn compact(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.chars().count() <= 160 {
        return text;
    }
    let mut truncated: String = text.chars().take(160).collect();
    truncated.push('…');
    truncated
}
