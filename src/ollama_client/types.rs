//! The types the rest of the crate speaks: conversation messages, tool calls
//! and tool definitions. Nothing here knows about HTTP.

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

impl Role {
    /// Wire name shared by both dialects.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
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

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: None,
            tool_call_id: None,
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
