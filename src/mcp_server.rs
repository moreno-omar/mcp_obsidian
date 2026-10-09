//! MCP interface for the Obsidian vault.
//!
//! The four tools required by `SPEC.md` are exposed here as MCP tools:
//! `search_by_tag`, `search_by_term`, `get_linked_notes` and `get_note`.
//!
//! The request/response structs and the `run_*` functions are the single source
//! of truth shared by:
//!
//! * the MCP tool handlers (`#[tool]` methods, used by MCP hosts), and
//! * [`ObsidianServer::tool_specs`] / [`ObsidianServer::dispatch`], used by the
//!   local host loop in [`crate::host`].

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::ollama_client::ToolSpec;
use crate::vault::{DEFAULT_BODY_CHARS, DEFAULT_EXCERPT_CHARS, DEFAULT_LIMIT, Direction, Vault};

/// Guidance shared by the MCP `instructions` field and the host system prompt.
///
/// The first paragraph is deliberately imperative: local models otherwise
/// answer from their own knowledge and never call a tool at all.
pub const SERVER_INSTRUCTIONS: &str = "\
This server exposes a read-only Obsidian markdown vault. Always search the \
vault before answering a question about its contents: never answer from your \
own knowledge, and never tell the user that the vault has nothing on a topic \
before you have searched it. Use search_by_tag for topical lookup, \
search_by_term for full-text lookup, get_linked_notes to follow the wikilink \
graph (links and backlinks) and get_note to read a specific note in full. Note \
paths returned by the tools can be passed back as the `note` argument of \
get_linked_notes or get_note. Answer from the returned notes only, cite the \
note path(s) you used, and say so plainly when nothing matches.";

fn default_limit() -> usize {
    DEFAULT_LIMIT
}

fn default_excerpt_chars() -> usize {
    DEFAULT_EXCERPT_CHARS
}

fn default_body_chars() -> usize {
    DEFAULT_BODY_CHARS
}

fn default_depth() -> usize {
    1
}

fn default_direction() -> String {
    "both".to_string()
}

/// Arguments for `search_by_tag`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SearchByTagRequest {
    /// Tags to look up, without the leading '#'. A tag also matches nested tags.
    pub tags: Vec<String>,
    /// Require notes to carry every requested tag instead of any of them.
    #[serde(default)]
    pub match_all: bool,
    /// Maximum number of notes to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Characters of note body included per result.
    #[serde(default = "default_excerpt_chars")]
    pub max_chars: usize,
}

/// Arguments for `search_by_term`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SearchByTermRequest {
    /// Search terms, matched against title, tags, headings, frontmatter and body.
    pub terms: Vec<String>,
    /// Require every term to match instead of any of them.
    #[serde(default)]
    pub match_all: bool,
    /// Maximum number of notes to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Characters of note body included per result.
    #[serde(default = "default_excerpt_chars")]
    pub max_chars: usize,
}

/// Arguments for `get_linked_notes`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct LinkedNotesRequest {
    /// Note name, file stem, frontmatter alias or vault-relative path.
    pub note: String,
    /// Hops to traverse through the wikilink graph.
    #[serde(default = "default_depth")]
    pub depth: usize,
    /// Graph direction: `outgoing`, `backlinks` or `both`.
    #[serde(default = "default_direction")]
    pub direction: String,
    /// Maximum number of notes to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Characters of note body included per result.
    #[serde(default = "default_excerpt_chars")]
    pub max_chars: usize,
}

/// Arguments for `get_note`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetNoteRequest {
    /// Note name, file stem, frontmatter alias or vault-relative path.
    pub note: String,
    /// Maximum characters of the note body to return.
    #[serde(default = "default_body_chars")]
    pub max_chars: usize,
}

/// MCP server (and in-process tool registry) over an indexed vault.
#[derive(Clone)]
pub struct ObsidianServer {
    vault: Arc<Vault>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ObsidianServer {
    /// Create a server over an already indexed vault.
    pub fn new(vault: Arc<Vault>) -> Self {
        Self {
            vault,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Search the vault by tag. Call this before answering any question about a topic; tags also match nested tags (e.g. 'project' matches 'project/alpha')."
    )]
    fn search_by_tag(&self, Parameters(request): Parameters<SearchByTagRequest>) -> String {
        log_tool_call("search_by_tag", &request);
        run_search_by_tag(&self.vault, &request)
    }

    #[tool(
        description = "Search the vault full-text across title, tags, headings, frontmatter and body. Start here for most questions: pass the subject words as separate terms, e.g. [\"list\", \"comprehensions\", \"python\"]."
    )]
    fn search_by_term(&self, Parameters(request): Parameters<SearchByTermRequest>) -> String {
        log_tool_call("search_by_term", &request);
        run_search_by_term(&self.vault, &request)
    }

    #[tool(
        description = "Follow the wikilink graph from a note: its outgoing links, its backlinks, or both, up to N hops. Use a path returned by another tool as `note`."
    )]
    fn get_linked_notes(&self, Parameters(request): Parameters<LinkedNotesRequest>) -> String {
        log_tool_call("get_linked_notes", &request);
        run_get_linked_notes(&self.vault, &request)
    }

    #[tool(
        description = "Read one note in full, with its metadata header. Use the paths returned by the search tools."
    )]
    fn get_note(&self, Parameters(request): Parameters<GetNoteRequest>) -> String {
        log_tool_call("get_note", &request);
        run_get_note(&self.vault, &request)
    }
}

// The macro needs a literal, so this short form is kept in sync by hand with
// `SERVER_INSTRUCTIONS`, which the local host loop uses as its system prompt.
#[tool_handler(
    router = self.tool_router,
    name = "mcp-obsidian",
    version = "0.1.0",
    instructions = "Read-only, deterministic retrieval over the user's Obsidian vault (tags, full-text terms, wikilink graph). Always call a tool before answering a question about the vault's contents; never answer from your own knowledge, and cite the note paths you used."
)]
impl ServerHandler for ObsidianServer {}

impl ObsidianServer {
    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    /// Tool definitions (name, description, JSON Schema) for LLM tool calling.
    ///
    /// Schemas are generated from the same structs the MCP tools deserialize,
    /// so both interfaces can never drift apart.
    pub fn tool_specs() -> Vec<ToolSpec> {
        vec![
            ToolSpec::new(
                "search_by_tag",
                "Search the vault by tag. Call this before answering any question about a topic; tags also match nested tags (e.g. 'project' matches 'project/alpha').",
                schema_of::<SearchByTagRequest>(),
            ),
            ToolSpec::new(
                "search_by_term",
                "Search the vault full-text across title, tags, headings, frontmatter and body. Start here for most questions: pass the subject words as separate terms, e.g. [\"list\", \"comprehensions\", \"python\"].",
                schema_of::<SearchByTermRequest>(),
            ),
            ToolSpec::new(
                "get_linked_notes",
                "Follow the wikilink graph from a note: outgoing links, backlinks or both, up to N hops. Use a path returned by another tool as `note`.",
                schema_of::<LinkedNotesRequest>(),
            ),
            ToolSpec::new(
                "get_note",
                "Read one note in full, with its metadata header. Use the paths returned by the search tools.",
                schema_of::<GetNoteRequest>(),
            ),
        ]
    }

    /// Execute a tool by name. Used by the local host loop; never panics on bad
    /// input, it returns an error the model can read and correct.
    pub fn dispatch(&self, name: &str, arguments: &Value) -> anyhow::Result<String> {
        match name {
            "search_by_tag" => {
                let request: SearchByTagRequest = parse_arguments(name, arguments)?;
                Ok(run_search_by_tag(&self.vault, &request))
            }
            "search_by_term" => {
                let request: SearchByTermRequest = parse_arguments(name, arguments)?;
                Ok(run_search_by_term(&self.vault, &request))
            }
            "get_linked_notes" => {
                let request: LinkedNotesRequest = parse_arguments(name, arguments)?;
                Ok(run_get_linked_notes(&self.vault, &request))
            }
            "get_note" => {
                let request: GetNoteRequest = parse_arguments(name, arguments)?;
                Ok(run_get_note(&self.vault, &request))
            }
            other => anyhow::bail!("unknown tool '{other}'"),
        }
    }
}

/// Log one MCP tool call, so `serve -v` shows what an external host asked for.
fn log_tool_call<T: std::fmt::Debug>(name: &str, request: &T) {
    tracing::debug!(tool = name, args = %format!("{request:?}"), "mcp tool call");
}

/// Deserialize tool arguments, reporting the tool name on failure.
fn parse_arguments<T: serde::de::DeserializeOwned>(
    tool: &str,
    arguments: &Value,
) -> anyhow::Result<T> {
    serde_json::from_value(arguments.clone())
        .map_err(|error| anyhow::anyhow!("invalid arguments for {tool}: {error}"))
}

/// JSON Schema for a tool argument struct.
fn schema_of<T: rmcp::schemars::JsonSchema>() -> Value {
    serde_json::to_value(rmcp::schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// Shared implementation of `search_by_tag`.
pub fn run_search_by_tag(vault: &Vault, request: &SearchByTagRequest) -> String {
    if request.tags.is_empty() {
        return "ERROR: `tags` must contain at least one tag.".to_string();
    }
    let hits = vault.search_by_tag(&request.tags, request.match_all, request.limit);
    render(
        vault,
        &hits,
        request.max_chars,
        "tag",
        &request.tags.join(", "),
    )
}

/// Shared implementation of `search_by_term`.
pub fn run_search_by_term(vault: &Vault, request: &SearchByTermRequest) -> String {
    if request.terms.is_empty() {
        return "ERROR: `terms` must contain at least one search term.".to_string();
    }
    let hits = vault.search_by_term(&request.terms, request.match_all, request.limit);
    render(
        vault,
        &hits,
        request.max_chars,
        "term",
        &request.terms.join(", "),
    )
}

/// Shared implementation of `get_linked_notes`.
pub fn run_get_linked_notes(vault: &Vault, request: &LinkedNotesRequest) -> String {
    let direction = match request.direction.parse::<Direction>() {
        Ok(direction) => direction,
        Err(error) => return format!("ERROR: {error}"),
    };
    match vault.get_linked_notes(&request.note, request.depth, direction, request.limit) {
        Ok(hits) => render(vault, &hits, request.max_chars, "graph", &request.note),
        Err(error) => format!("ERROR: {error}"),
    }
}

/// Shared implementation of `get_note`.
pub fn run_get_note(vault: &Vault, request: &GetNoteRequest) -> String {
    match vault.get_note(&request.note) {
        Ok(note) => vault.render_note(note, request.max_chars),
        Err(error) => format!("ERROR: {error}"),
    }
}

/// Render results, prefixing a deterministic header so the model can tell an
/// empty result from a failed one.
fn render(
    vault: &Vault,
    hits: &[crate::vault::Hit],
    max_chars: usize,
    kind: &str,
    query: &str,
) -> String {
    if hits.is_empty() {
        return format!("No notes matched the {kind} query '{query}'.");
    }
    let header = format!(
        "{} note(s) matched the {kind} query '{query}':\n",
        hits.len()
    );
    format!("{header}\n{}", vault.render_hits(hits, max_chars))
}
