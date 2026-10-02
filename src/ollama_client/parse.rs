//! Either dialect's JSON response → [`AssistantTurn`].
//!
//! The normalisation that matters: OpenAI returns tool arguments JSON encoded
//! as a *string* and always supplies a call id, native returns an object and
//! may omit the id. Missing ids are synthesised so the assistant message and
//! the matching tool result stay correlated.

use anyhow::{Context, Result};
use serde_json::Value;

use super::types::{AssistantTurn, ToolCall};

/// Parse an Ollama native (`/api/chat`) response body.
pub(super) fn parse_native_response(body: &Value) -> Result<AssistantTurn> {
    let message = body
        .get("message")
        .context("Ollama native response is missing `message`")?;
    Ok(AssistantTurn {
        content: text_field(message, "content"),
        tool_calls: tool_calls_of(message, false),
        finish_reason: optional_text(body, "done_reason"),
    })
}

/// Parse an OpenAI compatible (`/v1/chat/completions`) response body.
pub(super) fn parse_openai_response(body: &Value) -> Result<AssistantTurn> {
    let choice = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .context("OpenAI compatible response is missing `choices[0]`")?;
    let message = choice
        .get("message")
        .context("OpenAI compatible response is missing `choices[0].message`")?;
    Ok(AssistantTurn {
        content: text_field(message, "content"),
        tool_calls: tool_calls_of(message, true),
        finish_reason: optional_text(choice, "finish_reason"),
    })
}

/// Normalise tool calls from either flavour.
///
/// `arguments_as_string` selects the OpenAI shape, where arguments arrive JSON
/// encoded.
fn tool_calls_of(message: &Value, arguments_as_string: bool) -> Vec<ToolCall> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|calls| parse_tool_calls(calls, arguments_as_string))
        .unwrap_or_default()
}

/// Parse the `tool_calls` array into internal calls, skipping malformed entries.
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
                .unwrap_or_else(|| serde_json::json!({}));
            Some(ToolCall {
                id: call_id(call, position),
                name,
                arguments: parse_arguments_value(raw, arguments_as_string),
            })
        })
        .collect()
}

/// Decode arguments, tolerating both an already-parsed object and a JSON string.
///
/// A string that is not valid JSON is kept verbatim: the tool layer will report
/// a readable "invalid arguments" error instead of losing the model's text.
fn parse_arguments_value(raw: Value, arguments_as_string: bool) -> Value {
    if !arguments_as_string {
        return raw;
    }
    match raw {
        Value::String(text) if !text.trim().is_empty() => {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        }
        other => other,
    }
}

/// The call id, synthesised when the server omitted it.
fn call_id(call: &Value, position: usize) -> Option<String> {
    call.get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| Some(format!("call_{position}")))
}

/// A string field, defaulting to the empty string.
fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// An optional string field.
fn optional_text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}
