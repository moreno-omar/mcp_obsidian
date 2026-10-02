//! `mcp_obsidian` — deterministic, graph-aware retrieval over an Obsidian vault.
//!
//! The crate is split into layers so each one can be read on its own:
//!
//! | module | responsibility |
//! | :- | :- |
//! | [`vault`] | scan, index and query a markdown vault (no model involved) |
//! | [`ollama_client`] | HTTP access to a local Ollama, both wire dialects |
//! | [`mcp_server`] | the four MCP tools plus their shared in-process dispatch |
//! | [`host`] | the LLM ↔ vault loop used by `ask` and `chat` |
//!
//! `main.rs` is a thin CLI over these modules: the library owns all behaviour,
//! which is also what lets `tests/` exercise it without spawning the binary.

pub mod host;
pub mod mcp_server;
pub mod ollama_client;
pub mod vault;
