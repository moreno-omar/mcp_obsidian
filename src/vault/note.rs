//! The data model a caller sees: notes, stats, hits and graph direction.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use anyhow::{Result, bail};
use serde::Serialize;

/// A single indexed markdown note.
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    /// Vault-relative path using `/` separators, e.g. `projects/alpha.md`.
    pub rel_path: String,
    /// Frontmatter `title` when present, otherwise the file stem.
    pub title: String,
    /// Normalised tags (lowercase, no leading `#`), sorted.
    pub tags: BTreeSet<String>,
    /// Outgoing wikilink targets, exactly as written in the note.
    pub links: BTreeSet<String>,
    /// Headings in document order (text only, no `#`).
    pub headings: Vec<String>,
    /// Flattened frontmatter: key -> values.
    pub frontmatter: BTreeMap<String, Vec<String>>,
    /// Markdown body with the frontmatter block removed.
    pub body: String,
}

/// Aggregate counters describing an indexed vault.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct VaultStats {
    pub notes: usize,
    pub tags: usize,
    pub headings: usize,
    pub links: usize,
    pub unresolved_links: usize,
}

/// A ranked retrieval result.
#[derive(Debug, Clone)]
pub struct Hit {
    /// Index into the vault's note vector, used to render the hit.
    pub index: usize,
    pub score: u32,
    /// Human-readable justification, e.g. `tag match: project`.
    pub reason: String,
}

/// Which direction of the link graph to traverse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Notes this note links to.
    Outgoing,
    /// Notes that link to this note.
    Backlinks,
    /// Both directions.
    Both,
}

impl Direction {
    /// Whether [`Direction::Outgoing`] edges are included.
    pub(crate) fn includes_outgoing(self) -> bool {
        matches!(self, Self::Outgoing | Self::Both)
    }

    /// Whether [`Direction::Backlinks`] edges are included.
    pub(crate) fn includes_backlinks(self) -> bool {
        matches!(self, Self::Backlinks | Self::Both)
    }
}

impl FromStr for Direction {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "out" | "outgoing" | "links" => Ok(Self::Outgoing),
            "in" | "back" | "backlinks" => Ok(Self::Backlinks),
            "both" | "all" => Ok(Self::Both),
            other => bail!("unknown direction '{other}' (expected outgoing, backlinks or both)"),
        }
    }
}
