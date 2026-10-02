//! Internal types → JSON request bodies.
//!
//! The two dialects differ in three places only: the shape of `tool_calls`,
//! whether tool arguments are an object or a JSON string, and whether the
//! `tool_call_id` is echoed back. Everything else is shared.

use serde_json::{Value, json};

use super::types::{ApiFlavor, ChatMessage, Role, ToolSpec};

/// The request body for one chat round in the given dialect.
pub(super) fn request_body(
    api: ApiFlavor,
    model: &str,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
) -> Value {
    let wire: Vec<Value> = match api {
        ApiFlavor::Native => messages.iter().map(message_to_native).collect(),
        ApiFlavor::Openai => messages.iter().map(message_to_openai).collect(),
    };
    let mut request = json!({
        "model": model,
        "messages": wire,
        "stream": false,
    });
    if !tools.is_empty() {
        request["tools"] = Value::Array(tools.iter().map(ToolSpec::to_json).collect());
    }
    request
}

/// Trim a trailing slash and an optional `/v1` suffix so endpoints can be
/// derived from a single canonical root.
pub(super) fn normalize_root_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

/// Ollama native message: arguments stay an object, no call id, `tool_name`.
fn message_to_native(message: &ChatMessage) -> Value {
    let mut value = json!({
        "role": message.role.as_str(),
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

/// OpenAI compatible message: arguments as a JSON string, plus a call id.
fn message_to_openai(message: &ChatMessage) -> Value {
    let mut value = json!({
        "role": message.role.as_str(),
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

/// OpenAI expects tool arguments as a JSON encoded string.
fn arguments_to_string(arguments: &Value) -> String {
    match arguments {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
