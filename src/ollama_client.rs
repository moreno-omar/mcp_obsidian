//! HTTP access to a local Ollama instance.
//!
//! Two wire formats are supported and normalised into the same internal types:
//!
//! * **OpenAI compatible** — `POST {base}/v1/chat/completions`
//!   (`tool_calls[].function.arguments` is a JSON *string*).
//! * **Ollama native** — `POST {base}/api/chat`
//!   (`tool_calls[].function.arguments` is a JSON *object*).
//!
//! Everything above this module only ever sees [`ChatMessage`], [`ToolSpec`],
//! [`ToolCall`] and [`AssistantTurn`].

use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Where Ollama listens by default (OpenAI compatible endpoint).
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
/// Model used by Phase 1 (`ollama list` shows it supports tools).
pub const DEFAULT_MODEL: &str = "gemma4:e4b";
/// Local inference is slow; allow a generous request timeout.
pub const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// Which HTTP API shape to speak to Ollama.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ApiFlavor {
    /// `POST /v1/chat/completions` (OpenAI compatible).
    Openai,
    /// `POST /api/chat` (Ollama native).
    Native,
}

impl ApiFlavor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Native => "native",
        }
    }
}

/// Conversation role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One entry of the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Tool name, echoed back with `role = tool` results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// OpenAI requires the id of the call being answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::plain(Role::User, content)
    }

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: None,
            tool_call_id: None,
        }
    }

    /// Assistant turn that requested one or more tool calls.
    pub fn assistant_tool_calls(content: Option<String>, tool_calls: &[ToolCall]) -> Self {
        Self {
            role: Role::Assistant,
            content: content.unwrap_or_default(),
            tool_calls: tool_calls.to_vec(),
            tool_name: None,
            tool_call_id: None,
        }
    }

    /// Result of executing a tool, fed back to the model.
    pub fn tool_result(call: &ToolCall, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: Some(call.name.clone()),
            tool_call_id: call.id.clone(),
        }
    }
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Correlation id; synthesised when the server omits it.
    pub id: Option<String>,
    pub name: String,
    /// Already-parsed arguments (object for native, parsed string for OpenAI).
    pub arguments: Value,
}

/// The model's reply for a single round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantTurn {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    /// `stop`, `length`, or the raw finish reason when reported.
    pub finish_reason: Option<String>,
}

impl AssistantTurn {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// A tool definition advertised to the model.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool arguments.
    pub parameters: Value,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }

    /// Wire representation shared by both API flavours.
    pub fn to_json(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// Minimal HTTP client for a local Ollama server.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    /// Server root, e.g. `http://localhost:11434` (never includes `/v1`).
    root_url: String,
    api: ApiFlavor,
    model: String,
}

impl OllamaClient {
    /// Build a client. `base_url` may include the `/v1` suffix or not.
    pub fn new(
        base_url: impl Into<String>,
        api: ApiFlavor,
        model: impl Into<String>,
    ) -> Result<Self> {
        let root_url = normalize_root_url(&base_url.into());
        if root_url.is_empty() {
            bail!("ollama base url must not be empty");
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            http,
            root_url,
            api,
            model: model.into(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.root_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn api(&self) -> ApiFlavor {
        self.api
    }

    /// Ollama native chat endpoint.
    pub fn native_url(&self) -> String {
        format!("{}/api/chat", self.root_url)
    }

    /// OpenAI compatible chat endpoint.
    pub fn openai_url(&self) -> String {
        format!("{}/v1/chat/completions", self.root_url)
    }

    /// URL used for chat completions.
    pub fn chat_url(&self) -> String {
        match self.api {
            ApiFlavor::Native => self.native_url(),
            ApiFlavor::Openai => self.openai_url(),
        }
    }

    /// URL used for the reachability probe.
    pub fn version_url(&self) -> String {
        format!("{}/api/version", self.root_url)
    }

    /// Report the Ollama server version.
    pub async fn version(&self) -> Result<String> {
        let response = self
            .http
            .get(self.version_url())
            .send()
            .await
            .with_context(|| format!("cannot reach Ollama at {}", self.version_url()))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Ollama returned HTTP {status} for {}", self.version_url());
        }
        let body: Value = response
            .json()
            .await
            .context("Ollama version response was not valid JSON")?;
        Ok(body
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string())
    }

    /// List locally available model names.
    pub async fn models(&self) -> Result<Vec<String>> {
        let native = format!("{}/api/tags", self.root_url);
        if let Ok(response) = self.http.get(&native).send().await
            && response.status().is_success()
        {
            let body: Value = response
                .json()
                .await
                .context("Ollama /api/tags response was not valid JSON")?;
            if let Some(entries) = body.get("models").and_then(Value::as_array) {
                let mut names: Vec<String> = entries
                    .iter()
                    .filter_map(|entry| {
                        entry
                            .get("model")
                            .or_else(|| entry.get("name"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect();
                names.sort();
                names.dedup();
                return Ok(names);
            }
        }

        let openai = format!("{}/v1/models", self.root_url);
        let response = self
            .http
            .get(&openai)
            .send()
            .await
            .with_context(|| format!("cannot list models at {native} or {openai}"))?;
        let body: Value = response
            .json()
            .await
            .context("model list response was not valid JSON")?;
        let mut names: Vec<String> = body
            .get("data")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// `true` when `model` is present locally, tolerating an implicit `:latest`.
    pub fn model_available(models: &[String], model: &str) -> bool {
        let wanted = model.trim();
        if wanted.is_empty() {
            return false;
        }
        models.iter().any(|candidate| {
            candidate == wanted
                || candidate == &format!("{wanted}:latest")
                || candidate
                    .strip_suffix(":latest")
                    .map(|base| base == wanted)
                    .unwrap_or(false)
        })
    }

    /// Send the conversation (and optional tool definitions) to the model.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<AssistantTurn> {
        let request = match self.api {
            ApiFlavor::Native => self.native_request(messages, tools),
            ApiFlavor::Openai => self.openai_request(messages, tools),
        };
        let url = self.chat_url();
        let response = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("cannot reach Ollama at {url}"))?;

        let status = response.status();
        let body: Value = response.json().await.with_context(|| {
            format!("Ollama returned a non-JSON response from {url} (HTTP {status})")
        })?;
        if !status.is_success() {
            let detail = body
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| body.to_string());
            bail!("Ollama error (HTTP {status}): {detail}");
        }

        match self.api {
            ApiFlavor::Native => parse_native_response(&body),
            ApiFlavor::Openai => parse_openai_response(&body),
        }
    }

    fn native_request(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Value {
        let wire: Vec<Value> = messages.iter().map(message_to_native).collect();
        let mut request = json!({
            "model": self.model,
            "messages": wire,
            "stream": false,
        });
        if !tools.is_empty() {
            request["tools"] = Value::Array(tools.iter().map(ToolSpec::to_json).collect());
        }
        request
    }

    fn openai_request(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Value {
        let wire: Vec<Value> = messages.iter().map(message_to_openai).collect();
        let mut request = json!({
            "model": self.model,
            "messages": wire,
            "stream": false,
        });
        if !tools.is_empty() {
            request["tools"] = Value::Array(tools.iter().map(ToolSpec::to_json).collect());
        }
        request
    }
}

/// Trim a trailing slash and an optional `/v1` suffix so endpoints can be
/// derived from a single canonical root.
fn normalize_root_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

fn message_to_native(message: &ChatMessage) -> Value {
    let mut value = json!({
        "role": role_str(message.role),
        "content": message.content,
    });
    if !message.tool_calls.is_empty() {
        let calls: Vec<Value> = message
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "function": {
                        "name": call.name,
                        "arguments": call.arguments,
                    }
                })
            })
            .collect();
        value["tool_calls"] = Value::Array(calls);
    }
    if let Some(name) = &message.tool_name {
        value["tool_name"] = Value::String(name.clone());
    }
    value
}

fn message_to_openai(message: &ChatMessage) -> Value {
    let mut value = json!({
        "role": role_str(message.role),
        "content": message.content,
    });
    if !message.tool_calls.is_empty() {
        let calls: Vec<Value> = message
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id.clone().unwrap_or_else(|| "call_0".to_string()),
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": arguments_to_string(&call.arguments),
                    }
                })
            })
            .collect();
        value["tool_calls"] = Value::Array(calls);
    }
    if message.role == Role::Tool
        && let Some(id) = &message.tool_call_id
    {
        value["tool_call_id"] = Value::String(id.clone());
    }
    value
}

fn role_str(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// OpenAI expects tool arguments as a JSON encoded string.
fn arguments_to_string(arguments: &Value) -> String {
    match arguments {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn parse_native_response(body: &Value) -> Result<AssistantTurn> {
    let message = body
        .get("message")
        .context("Ollama native response is missing `message`")?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| parse_tool_calls(calls, false))
        .unwrap_or_default();
    let finish_reason = body
        .get("done_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(AssistantTurn {
        content,
        tool_calls,
        finish_reason,
    })
}

fn parse_openai_response(body: &Value) -> Result<AssistantTurn> {
    let choice = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .context("OpenAI compatible response is missing `choices[0]`")?;
    let message = choice
        .get("message")
        .context("OpenAI compatible response is missing `choices[0].message`")?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tool_calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| parse_tool_calls(calls, true))
        .unwrap_or_default();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(AssistantTurn {
        content,
        tool_calls,
        finish_reason,
    })
}

/// Normalise tool calls from either flavour.
///
/// `arguments_as_string` selects the OpenAI shape, where arguments arrive JSON
/// encoded. Missing call ids are synthesised so the assistant message and the
/// matching tool result stay correlated.
fn parse_tool_calls(calls: &[Value], arguments_as_string: bool) -> Vec<ToolCall> {
    calls
        .iter()
        .enumerate()
        .filter_map(|(position, call)| {
            let function = call.get("function")?;
            let name = function.get("name")?.as_str()?.to_string();
            let raw = function
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let arguments = if arguments_as_string {
                match raw {
                    Value::String(text) if !text.trim().is_empty() => {
                        serde_json::from_str(&text).unwrap_or(Value::String(text))
                    }
                    other => other,
                }
            } else {
                raw
            };
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| Some(format!("call_{position}")));
            Some(ToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}
