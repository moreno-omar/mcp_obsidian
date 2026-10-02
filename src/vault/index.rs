//! The [`Vault`] type: scan the directory once, build every index, resolve names.
//!
//! Index building is one pass over the parsed notes, producing the structures
//! the queries in [`super::query`] need:
//!
//! | index | purpose |
//! | :- | :- |
//! | `aliases` | path / file stem / frontmatter alias → note |
//! | `tag_index` | tag → notes (also serves nested tags) |
//! | `term_index` | lowercased word → notes (pre-filter so no full scan) |
//! | `outgoing` / `incoming` | resolved wikilink graph, both directions |

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::note::{Direction, Hit, Note, VaultStats};
use super::parse::{note_aliases, parse_note};
use super::text::{collect_terms, normalize_key};
use super::walk::{markdown_files, read_note};

/// An in-memory index of an Obsidian vault.
#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    /// All notes, sorted by `rel_path` for reproducible iteration order.
    notes: Vec<Note>,
    /// Normalised alias -> note indices.
    aliases: BTreeMap<String, Vec<usize>>,
    /// Tag (and nested tag prefixes) -> note indices.
    tag_index: BTreeMap<String, Vec<usize>>,
    /// Lowercased word -> note indices, used to pre-filter term searches.
    term_index: BTreeMap<String, Vec<usize>>,
    /// Resolved outgoing links per note.
    outgoing: Vec<BTreeSet<usize>>,
    /// Resolved incoming links (backlinks) per note.
    incoming: Vec<BTreeSet<usize>>,
    /// Link targets that could not be resolved to a file.
    unresolved: BTreeSet<String>,
}

impl Vault {
    /// Scan `root` and build the full in-memory index.
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let requested = root.as_ref();
        let root = requested
            .canonicalize()
            .with_context(|| format!("vault path not found: {}", requested.display()))?;
        if !root.is_dir() {
            bail!("vault path is not a directory: {}", root.display());
        }

        let mut notes = Vec::new();
        for path in markdown_files(&root)? {
            let rel_path = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            notes.push(parse_note(&rel_path, &read_note(&path)?));
        }

        Ok(Self::build(root, notes))
    }

    /// Build all derived indices from parsed notes.
    ///
    /// Note that `notes` is already sorted, so every index ends up in a
    /// deterministic order without a second sort pass.
    fn build(root: PathBuf, notes: Vec<Note>) -> Self {
        let count = notes.len();
        let mut vault = Self {
            root,
            notes,
            aliases: BTreeMap::new(),
            tag_index: BTreeMap::new(),
            term_index: BTreeMap::new(),
            outgoing: vec![BTreeSet::new(); count],
            incoming: vec![BTreeSet::new(); count],
            unresolved: BTreeSet::new(),
        };

        vault.index_notes();
        vault.resolve_links(count);
        vault
    }

    /// Alias, tag and term postings lists, one note at a time.
    fn index_notes(&mut self) {
        for index in 0..self.notes.len() {
            for alias in note_aliases(&self.notes[index]) {
                self.aliases.entry(alias).or_default().push(index);
            }
            for tag in &self.notes[index].tags {
                self.tag_index.entry(tag.clone()).or_default().push(index);
            }
            for term in note_terms(&self.notes[index]) {
                self.term_index.entry(term).or_default().push(index);
            }
        }
    }

    /// Turn wikilink targets into the outgoing/incoming edge sets.
    ///
    /// Targets that resolve to nothing are recorded in `unresolved` (they are
    /// what `doctor` reports as link rot) and self-links are dropped.
    fn resolve_links(&mut self, count: usize) {
        for index in 0..count {
            let targets: Vec<String> = self.notes[index].links.iter().cloned().collect();
            for target in targets {
                let resolved = self.resolve_indices(&target);
                if resolved.is_empty() {
                    self.unresolved.insert(target);
                    continue;
                }
                for target_index in resolved {
                    if target_index == index {
                        continue;
                    }
                    self.outgoing[index].insert(target_index);
                    self.incoming[target_index].insert(index);
                }
            }
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All notes in `rel_path` order.
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// One note by index, for callers holding [`Hit`] indices.
    pub(crate) fn note_at(&self, index: usize) -> &Note {
        &self.notes[index]
    }

    /// Aggregate counters for the indexed vault.
    pub fn stats(&self) -> VaultStats {
        VaultStats {
            notes: self.notes.len(),
            tags: self.tag_index.len(),
            headings: self.notes.iter().map(|note| note.headings.len()).sum(),
            links: self.notes.iter().map(|note| note.links.len()).sum(),
            unresolved_links: self.unresolved.len(),
        }
    }

    /// Link targets that referenced a non-existent note.
    pub fn unresolved_links(&self) -> &BTreeSet<String> {
        &self.unresolved
    }

    // -- Name resolution ----------------------------------------------------

    /// Resolve a note name (path, file stem or frontmatter alias) to indices.
    ///
    /// Exact aliases win. Only when nothing matches exactly do we fall back to a
    /// substring match on path/title, which keeps LLM guessing ("beta" for
    /// "beta-note") useful without ever shadowing an exact name.
    pub(crate) fn resolve_indices(&self, name: &str) -> Vec<usize> {
        let key = normalize_key(name);
        if key.is_empty() {
            return Vec::new();
        }
        if let Some(indices) = self.aliases.get(&key) {
            return indices.clone();
        }
        if let Some((_, basename)) = key.rsplit_once('/')
            && let Some(indices) = self.aliases.get(basename)
        {
            return indices.clone();
        }
        self.notes
            .iter()
            .enumerate()
            .filter(|(_, note)| {
                note.rel_path.to_lowercase().contains(&key)
                    || note.title.to_lowercase().contains(&key)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Resolve exactly one note, failing on unknown or ambiguous names.
    pub(crate) fn resolve_one(&self, name: &str) -> Result<usize> {
        let indices = self.resolve_indices(name);
        match indices.len() {
            0 => bail!("no note matches '{name}'"),
            1 => Ok(indices[0]),
            _ => {
                let candidates: Vec<&str> = indices
                    .iter()
                    .map(|index| self.notes[*index].rel_path.as_str())
                    .collect();
                bail!(
                    "note name '{name}' is ambiguous, use a full path: {}",
                    candidates.join(", ")
                )
            }
        }
    }

    /// Fetch a note by name (path, file stem or frontmatter alias).
    pub fn get_note(&self, name: &str) -> Result<&Note> {
        Ok(&self.notes[self.resolve_one(name)?])
    }

    // -- Graph access -------------------------------------------------------

    /// Notes carrying `tag`, directly or as a nested tag prefix.
    pub(crate) fn notes_with_tag(&self, tag: &str) -> BTreeSet<usize> {
        let prefix = format!("{tag}/");
        let mut matched = BTreeSet::new();
        for (candidate, indices) in &self.tag_index {
            if candidate == tag || candidate.starts_with(&prefix) {
                matched.extend(indices.iter().copied());
            }
        }
        matched
    }

    /// Note indices whose inverted-index entry contains `word`.
    pub(crate) fn notes_with_word(&self, word: &str) -> &[usize] {
        self.term_index.get(word).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Resolved neighbours of a note in the requested direction(s), sorted.
    ///
    /// Each entry carries the edge kind so the caller can explain the hit
    /// ("link of 'a.md'" vs "backlink of 'a.md'").
    pub(crate) fn neighbours(&self, index: usize, direction: Direction) -> Vec<(usize, &'static str)> {
        let mut result = Vec::new();
        if direction.includes_outgoing() {
            result.extend(self.outgoing[index].iter().map(|target| (*target, "link")));
        }
        if direction.includes_backlinks() {
            result.extend(self.incoming[index].iter().map(|source| (*source, "backlink")));
        }
        result.sort();
        result
    }

    /// Notes that link to `note`, resolved to paths.
    pub fn backlink_paths(&self, note: &Note) -> Vec<String> {
        self.related_paths(&self.incoming, note)
    }

    /// Outgoing links of `note` that resolved to real files.
    pub fn resolved_link_paths(&self, note: &Note) -> Vec<String> {
        self.related_paths(&self.outgoing, note)
    }

    // -- Ranking ------------------------------------------------------------

    pub(crate) fn hit(&self, index: usize, score: u32, reason: impl Into<String>) -> Hit {
        Hit {
            index,
            score,
            reason: reason.into(),
        }
    }

    /// Sort by score (desc) then path (asc) and truncate — deterministic.
    pub(crate) fn rank(&self, hits: impl IntoIterator<Item = Hit>, limit: usize) -> Vec<Hit> {
        let mut hits: Vec<Hit> = hits.into_iter().collect();
        hits.sort_by(|a, b| {
            b.score.cmp(&a.score).then_with(|| {
                self.notes[a.index]
                    .rel_path
                    .cmp(&self.notes[b.index].rel_path)
            })
        });
        if limit > 0 {
            hits.truncate(limit);
        }
        hits
    }

    /// Follow one edge direction from `note` and collect the neighbour paths.
    ///
    /// A note is matched by path rather than index because the public API hands
    /// out `&Note` (as returned by [`Vault::get_note`]).
    fn related_paths(&self, edges: &[BTreeSet<usize>], note: &Note) -> Vec<String> {
        let Some(index) = self
            .notes
            .iter()
            .position(|other| other.rel_path == note.rel_path)
        else {
            return Vec::new();
        };
        edges[index]
            .iter()
            .map(|other| self.notes[*other].rel_path.clone())
            .collect()
    }
}

/// Every word of a note that the inverted index should know about.
fn note_terms(note: &Note) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    collect_terms(&note.title, &mut terms);
    for tag in &note.tags {
        collect_terms(tag, &mut terms);
    }
    for heading in &note.headings {
        collect_terms(heading, &mut terms);
    }
    for value in note.frontmatter.values().flatten() {
        collect_terms(value, &mut terms);
    }
    collect_terms(&note.body, &mut terms);
    terms
}
