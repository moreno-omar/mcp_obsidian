//! Deterministic, graph-aware indexing of an Obsidian markdown vault.
//!
//! The vault is read **once** at startup and kept in memory. Every public query
//! is a pure function of that indexed state, so an identical vault plus an
//! identical query always produces byte-identical output (see `SPEC.md`:
//! "Output is deterministic and reproducible").
//!
//! Deliberately no vector embeddings / semantic search: retrieval is based on
//! tags, frontmatter, headings, body terms and the wikilink graph.
//!
//! # Module map
//!
//! | module | contents |
//! | :- | :- |
//! | [`note`] | public data model: [`Note`], [`Hit`], [`VaultStats`], [`Direction`] |
//! | [`index`] | [`Vault`] itself: scanning, index building, name resolution |
//! | [`walk`] | which files and directories a vault scan includes |
//! | [`parse`] | markdown → [`Note`]: frontmatter, headings, wikilinks, tags |
//! | [`query`] | the three retrieval queries and their ranking |
//! | [`render`] | hits and notes → LLM-friendly markdown |
//! | [`text`] | small pure string helpers shared by parsing, scoring and rendering |

mod index;
mod note;
mod parse;
mod query;
mod render;
mod text;
mod walk;

pub use index::Vault;
pub use note::{Direction, Hit, Note, VaultStats};

/// Default number of results returned by a search.
pub const DEFAULT_LIMIT: usize = 10;
/// Default body excerpt (in characters) attached to a search hit.
pub const DEFAULT_EXCERPT_CHARS: usize = 400;
/// Default body budget (in characters) for a fully rendered note.
pub const DEFAULT_BODY_CHARS: usize = 4_000;
