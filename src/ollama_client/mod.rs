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
//! [`ToolCall`] and [`AssistantTurn`], so the host loop in [`crate::host`] has
//! no dialect-specific branches.
//!
//! # Module map
//!
//! | module | contents |
//! | :- | :- |
//! | [`types`] | constants, [`ApiFlavor`], messages and tool types |
//! | [`client`] | [`OllamaClient`]: endpoints, reachability and `chat` |
//! | [`wire`] | internal types → JSON request bodies for both dialects |
//! | [`parse`] | either dialect's JSON response → [`AssistantTurn`] |

mod client;
mod parse;
mod types;
mod wire;

pub use client::OllamaClient;
pub use types::{
    ApiFlavor, AssistantTurn, ChatMessage, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_TIMEOUT_SECS,
    Role, ToolCall, ToolSpec,
};
