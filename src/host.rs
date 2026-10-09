//! Orchestration loop: the LLM ↔ vault bridge.
//!
//! This is *not* an MCP client connection. The tool layer is shared in-process
//! with the MCP server ([`ObsidianServer::dispatch`]), so the loop works without
//! a second process, pipes or a JSON-RPC round trip. `serve` still exposes the
//! very same tools to external MCP hosts over stdio.
//!
//! # Grounding
//!
//! Small local models are unreliable tool callers: asked about the vault they
//! often answer from their own knowledge instead of calling a tool, which is
//! exactly the failure the `ask` command used to hit. Two measures keep answers
//! grounded in the vault:
//!
//! 1. an imperative system prompt ([`SERVER_INSTRUCTIONS`] plus the vault size)
//!    that forbids answering before searching, and
//! 2. a deterministic pre-search (see `Host::prefetch_notes`): the prompt's
//!    subject words are run through the retrieval core *before* the model is
//!    called, and the hits are injected into the system prompt. Even a model
//!    that never calls a tool now sees the relevant notes.

use std::sync::Arc;

use anyhow::{Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::mcp_server::{ObsidianServer, SERVER_INSTRUCTIONS};
use crate::ollama_client::{ChatMessage, OllamaClient, ToolCall};
use crate::ui::{self, theme};

/// Hard cap on tool-call rounds so a confused model cannot loop forever.
pub const DEFAULT_MAX_ROUNDS: usize = 6;

/// Notes injected into the system prompt by the deterministic pre-search.
const PREFETCH_NOTES: usize = 3;
/// Characters of each pre-searched note injected into the system prompt.
const PREFETCH_CHARS: usize = 800;
/// Longest query the pre-search builds from the prompt.
const PREFETCH_MAX_TERMS: usize = 8;
/// Shortest word that is worth searching for.
const PREFETCH_MIN_WORD: usize = 3;

/// Words that carry no retrieval signal on their own.
const STOPWORDS: [&str; 42] = [
    "about", "after", "again", "all", "any", "are", "before", "can", "could", "does", "doing",
    "explain", "for", "from", "get", "give", "have", "here", "how", "into", "like", "more", "most",
    "much", "not", "note", "notes", "one", "only", "should", "some", "tell", "that", "their", "them",
    "then", "two", "use", "using", "want", "what", "when",
];

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
    /// Terms used by the deterministic pre-search, when one ran.
    pub prefetch: Option<String>,
}

/// Notes retrieved deterministically before the model is asked anything.
#[derive(Debug, Clone)]
struct Prefetch {
    /// The searched terms, rendered for the trace.
    query: String,
    /// Markdown payload injected into the system prompt.
    block: String,
}

/// Runs prompts against Ollama, executing vault tools on demand.
pub struct Host {
    client: OllamaClient,
    server: Arc<ObsidianServer>,
    use_tools: bool,
    prefetch: bool,
    max_rounds: usize,
}

impl Host {
    pub fn new(client: OllamaClient, server: Arc<ObsidianServer>) -> Self {
        Self {
            client,
            server,
            use_tools: true,
            prefetch: true,
            max_rounds: DEFAULT_MAX_ROUNDS,
        }
    }

    /// Disable tool calling (plain chat round trip, used for connectivity tests).
    pub fn without_tools(mut self) -> Self {
        self.use_tools = false;
        self
    }

    /// Disable the deterministic pre-search, so only model-requested tools run.
    pub fn without_prefetch(mut self) -> Self {
        self.prefetch = false;
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
        let prefetch = if self.prefetch && self.use_tools {
            self.prefetch_notes(prompt)
        } else {
            None
        };
        let mut messages = vec![
            ChatMessage::system(self.system_prompt(prefetch.as_ref())),
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
                    prefetch: prefetch.map(|prefetch| prefetch.query),
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
        );
    }

    /// Execute one tool call, converting failures into model-readable text.
    fn execute(&self, call: &ToolCall) -> String {
        tracing::info!(tool = %call.name, arguments = %compact(&call.arguments), "executing tool");
        match self.server.dispatch(&call.name, &call.arguments) {
            Ok(text) => text,
            Err(error) => format!("ERROR: {error}"),
        }
    }

    /// Search the vault for the prompt's subject words, before any model call.
    ///
    /// This is the safety net for models that refuse to call tools: the
    /// retrieval core is deterministic and instant, so the relevant notes can
    /// always be placed in front of the model. Returns `None` when the prompt
    /// has no words worth searching or nothing matches.
    fn prefetch_notes(&self, prompt: &str) -> Option<Prefetch> {
        let terms = search_terms(prompt);
        if terms.is_empty() {
            return None;
        }
        let hits = self
            .server
            .vault()
            .search_by_term(&terms, false, PREFETCH_NOTES);
        if hits.is_empty() {
            return None;
        }
        let block = self.server.vault().render_hits(&hits, PREFETCH_CHARS);
        Some(Prefetch {
            query: format!("[{}]", terms.join(", ")),
            block,
        })
    }

    /// System prompt: mandatory tool protocol, vault size and pre-search hits.
    fn system_prompt(&self, prefetch: Option<&Prefetch>) -> String {
        let mut prompt = format!(
            "{SERVER_INSTRUCTIONS}\n\nThe vault contains {} note(s).",
            self.server.vault().notes().len()
        );
        match prefetch {
            Some(prefetch) => prompt.push_str(&format!(
                "\n\nA deterministic pre-search for {} already returned these notes:\n\n{}\n\n\
                 Answer from them, call get_note with a path when an excerpt ends in \
                 `…[truncated]`, and call the search tools again if you need other notes.",
                prefetch.query, prefetch.block
            )),
            None => prompt.push_str(
                "\n\nNothing matched a pre-search, so search the vault yourself before answering.",
            ),
        }
        prompt
    }

    /// Interactive read-eval-print loop over stdin/stdout.
    pub async fn chat_loop(&self) -> Result<()> {
        let mut console = ui::stdout_console();
        ui::print_chat_banner(
            &mut console,
            self.client.model(),
            &self.client.chat_url(),
            &self.tool_names(),
        )?;

        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        loop {
            ui::write_user_prompt(&mut console)?;
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
                    if let Some(query) = &outcome.prefetch {
                        ui::print_prefetch(&mut console, query)?;
                    }
                    ui::print_trace(&mut console, &outcome.trace)?;
                    ui::print_line(&mut console, "llm>", theme::assistant_prompt())?;
                    ui::print_markdown(&mut console, outcome.answer.trim())?;
                    console.write("\n")?;
                }
                Err(error) => ui::print_error(&mut console, &format!("{error:#}"))?,
            }
        }
    }

    /// Names of the tools advertised to the model, for the startup banner.
    fn tool_names(&self) -> Vec<String> {
        if !self.use_tools {
            return Vec::new();
        }
        ObsidianServer::tool_specs()
            .into_iter()
            .map(tool_name)
            .collect()
    }
}

/// Name of a tool specification.
fn tool_name(spec: crate::ollama_client::ToolSpec) -> String {
    spec.name
}

/// Split a prompt into the distinct lowercase words worth searching for.
fn search_terms(prompt: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in prompt.split_whitespace() {
        let term = trim_punctuation(word);
        if term.chars().count() < PREFETCH_MIN_WORD {
            continue;
        }
        if STOPWORDS.contains(&term.as_str()) {
            continue;
        }
        if terms.contains(&term) {
            continue;
        }
        terms.push(term);
        if terms.len() == PREFETCH_MAX_TERMS {
            break;
        }
    }
    terms
}

/// Lowercase a word and drop punctuation and quotes around it.
fn trim_punctuation(word: &str) -> String {
    word.trim_matches(is_not_word_char).to_lowercase()
}

/// Characters that never belong to a searchable word.
fn is_not_word_char(ch: char) -> bool {
    !ch.is_alphanumeric() && ch != '-' && ch != '_'
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
