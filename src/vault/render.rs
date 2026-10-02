//! Rendering: hits and notes → deterministic, LLM-friendly markdown.
//!
//! Every function here writes plain markdown with a metadata header, so the
//! model can always see where the text came from. Nothing in this module
//! depends on a model being present.

use std::fmt::Write as _;

use super::index::Vault;
use super::note::{Hit, Note};
use super::text::{excerpt_of, truncate_chars};

impl Vault {
    /// Render search hits as an LLM-friendly markdown payload.
    pub fn render_hits(&self, hits: &[Hit], excerpt_chars: usize) -> String {
        if hits.is_empty() {
            return "No notes matched the query.".to_string();
        }
        let mut out = String::new();
        for hit in hits {
            let note = self.note_at(hit.index);
            let _ = writeln!(out, "## {}", note.title);
            let _ = writeln!(out, "- path: `{}`", note.rel_path);
            let _ = writeln!(out, "- score: {} ({})", hit.score, hit.reason);
            write_list(&mut out, "tags", tags_of(note));
            write_list(&mut out, "backlinks", self.backlink_paths(note));

            let excerpt = excerpt_of(&note.body, excerpt_chars);
            if !excerpt.is_empty() {
                let _ = writeln!(out, "\n{excerpt}");
            }
            out.push('\n');
        }
        out.trim_end().to_string()
    }

    /// Render one complete note (metadata header + body, truncated to budget).
    pub fn render_note(&self, note: &Note, body_chars: usize) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "# {}", note.title);
        let _ = writeln!(out, "- path: `{}`", note.rel_path);
        write_list(&mut out, "tags", tags_of(note));
        write_list(
            &mut out,
            "links",
            note.links.iter().map(|link| format!("[[{link}]]")),
        );
        write_list(&mut out, "resolved links", self.resolved_link_paths(note));
        write_list(&mut out, "backlinks", self.backlink_paths(note));
        write_joined(&mut out, "headings", note.headings.iter().cloned(), " | ");

        out.push('\n');
        out.push_str(&truncate_chars(note.body.trim(), body_chars));
        out
    }

    /// Deterministic one-line-per-note rendering of the whole index.
    pub fn render_index(&self, limit: usize) -> String {
        let stats = self.stats();
        let mut out = String::new();
        let _ = writeln!(out, "vault: {}", self.root().display());
        let _ = writeln!(
            out,
            "notes: {} | tags: {} | headings: {} | links: {} | unresolved links: {}",
            stats.notes, stats.tags, stats.headings, stats.links, stats.unresolved_links
        );
        if stats.notes > 0 {
            out.push('\n');
        }
        for note in self.notes().iter().take(limit_or_all(limit)) {
            let _ = writeln!(
                out,
                "{:<48} {:>3} links  {}",
                note.rel_path,
                note.links.len(),
                tags_of(note).join(" ")
            );
        }
        if limit > 0 && stats.notes > limit {
            let _ = writeln!(out, "… {} more notes", stats.notes - limit);
        }
        out.trim_end().to_string()
    }
}

/// Tags rendered the way they appear in Obsidian, e.g. `#project/alpha`.
fn tags_of(note: &Note) -> Vec<String> {
    note.tags.iter().map(|tag| format!("#{tag}")).collect()
}

/// Write a `- key: a, b` bullet, skipping empty values entirely.
fn write_list(out: &mut String, key: &str, values: impl IntoIterator<Item = String>) {
    write_joined(out, key, values, ", ");
}

/// Write a `- key: a<sep>b` bullet, skipping empty values entirely.
fn write_joined(
    out: &mut String,
    key: &str,
    values: impl IntoIterator<Item = String>,
    separator: &str,
) {
    let values: Vec<String> = values.into_iter().collect();
    if !values.is_empty() {
        let _ = writeln!(out, "- {key}: {}", values.join(separator));
    }
}

/// `0` means "no limit"; the iterator needs a concrete bound.
fn limit_or_all(limit: usize) -> usize {
    if limit == 0 { usize::MAX } else { limit }
}
