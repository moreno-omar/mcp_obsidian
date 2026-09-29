//! Deterministic, graph-aware indexing of an Obsidian markdown vault.
//!
//! The vault is read **once** at startup and kept in memory. Every public query
//! is a pure function of that indexed state, so an identical vault plus an
//! identical query always produces byte-identical output (see `SPEC.md`:
//! "Output is deterministic and reproducible").
//!
//! Deliberately no vector embeddings / semantic search: retrieval is based on
//! tags, frontmatter, headings, body terms and the wikilink graph.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use walkdir::WalkDir;

/// Default number of results returned by a search.
pub const DEFAULT_LIMIT: usize = 10;
/// Default body excerpt (in characters) attached to a search hit.
pub const DEFAULT_EXCERPT_CHARS: usize = 400;
/// Default body budget (in characters) for a fully rendered note.
pub const DEFAULT_BODY_CHARS: usize = 4_000;

/// Directories that never contain user notes and are pruned during the scan.
const SKIPPED_DIRS: [&str; 7] = [
    ".obsidian",
    ".git",
    ".trash",
    ".stversions",
    ".venv",
    "node_modules",
    "target",
];

/// Frontmatter keys that hold tags.
const TAG_KEYS: [&str; 2] = ["tags", "tag"];
/// Frontmatter keys that hold alternative note names.
const ALIAS_KEYS: [&str; 2] = ["aliases", "alias"];

// ---------------------------------------------------------------------------
// Public data model
// ---------------------------------------------------------------------------

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
    /// Index into [`Vault::notes`], used to render the hit.
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

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

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

        let mut files: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| !is_skipped_dir(entry))
        {
            let entry = entry.with_context(|| format!("failed to scan {}", root.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            if is_markdown(entry.path()) {
                files.push(entry.path().to_path_buf());
            }
        }
        files.sort();

        let mut notes = Vec::with_capacity(files.len());
        for path in files {
            let rel_path = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            notes.push(parse_note(&rel_path, &raw));
        }

        Ok(Self::build(root, notes))
    }

    /// Build all derived indices from parsed notes.
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

        for (index, note) in vault.notes.iter().enumerate() {
            for alias in note_aliases(note) {
                vault.aliases.entry(alias).or_default().push(index);
            }
            for tag in &note.tags {
                vault.tag_index.entry(tag.clone()).or_default().push(index);
            }
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
            for term in terms {
                vault.term_index.entry(term).or_default().push(index);
            }
        }

        for index in 0..count {
            let targets: Vec<String> = vault.notes[index].links.iter().cloned().collect();
            for target in targets {
                let resolved = vault.resolve_indices(&target);
                if resolved.is_empty() {
                    vault.unresolved.insert(target);
                    continue;
                }
                for target_index in resolved {
                    if target_index == index {
                        continue;
                    }
                    vault.outgoing[index].insert(target_index);
                    vault.incoming[target_index].insert(index);
                }
            }
        }

        vault
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All notes in `rel_path` order.
    pub fn notes(&self) -> &[Note] {
        &self.notes
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

    /// Resolve a note name (path, file stem or frontmatter alias) to indices.
    ///
    /// Exact aliases win. Only when nothing matches exactly do we fall back to a
    /// substring match on path/title, which keeps LLM guessing ("beta" for
    /// "beta-note") useful without ever shadowing an exact name.
    fn resolve_indices(&self, name: &str) -> Vec<usize> {
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
    fn resolve_one(&self, name: &str) -> Result<usize> {
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

    /// Search notes by tag. A query tag also matches nested tags
    /// (`project` matches `project/alpha`).
    pub fn search_by_tag(&self, tags: &[String], match_all: bool, limit: usize) -> Vec<Hit> {
        let queries: Vec<String> = tags
            .iter()
            .map(|tag| normalize_tag(tag))
            .filter(|tag| !tag.is_empty())
            .collect();
        if queries.is_empty() {
            return Vec::new();
        }

        let mut per_tag: Vec<BTreeSet<usize>> = Vec::with_capacity(queries.len());
        for query in &queries {
            per_tag.push(self.notes_with_tag(query));
        }

        let mut scores: BTreeMap<usize, u32> = BTreeMap::new();
        if match_all {
            let mut intersection = per_tag.first().cloned().unwrap_or_default();
            for set in per_tag.iter().skip(1) {
                intersection = intersection.intersection(set).copied().collect();
            }
            let score = 10 * queries.len() as u32;
            for index in intersection {
                scores.insert(index, score);
            }
        } else {
            for set in &per_tag {
                for index in set {
                    *scores.entry(*index).or_insert(0) += 10;
                }
            }
        }

        let reason = if queries.len() == 1 {
            format!("tag match: {}", queries[0])
        } else {
            format!("tag match: {}", queries.join(", "))
        };
        let hits = scores.into_iter().map(|(index, score)| {
            let mut score = score;
            if queries.iter().any(|query| {
                self.notes[index]
                    .title
                    .to_lowercase()
                    .contains(query.as_str())
            }) {
                score += 2;
            }
            self.hit(index, score, reason.clone())
        });
        self.rank(hits, limit)
    }

    /// Notes carrying `tag`, directly or as a nested tag prefix.
    fn notes_with_tag(&self, tag: &str) -> BTreeSet<usize> {
        let prefix = format!("{tag}/");
        let mut matched = BTreeSet::new();
        for (candidate, indices) in &self.tag_index {
            if candidate == tag || candidate.starts_with(&prefix) {
                matched.extend(indices.iter().copied());
            }
        }
        matched
    }

    /// Search notes by free-text term across title, tags, headings, frontmatter
    /// and body. With `match_all`, every term must match the note.
    pub fn search_by_term(&self, terms: &[String], match_all: bool, limit: usize) -> Vec<Hit> {
        let queries: Vec<String> = terms
            .iter()
            .map(|term| term.trim().to_lowercase())
            .filter(|term| !term.is_empty())
            .collect();
        if queries.is_empty() {
            return Vec::new();
        }

        // Pre-filter with the inverted index so we never scan every note body.
        let mut candidates = BTreeSet::new();
        for query in &queries {
            let mut words = BTreeSet::new();
            collect_terms(query, &mut words);
            for word in words {
                if let Some(indices) = self.term_index.get(&word) {
                    candidates.extend(indices.iter().copied());
                }
            }
        }

        let mut scored: Vec<Hit> = Vec::new();
        for index in candidates {
            let note = &self.notes[index];
            let mut total = 0u32;
            let mut matched_terms = 0usize;
            for query in &queries {
                let score = score_terms(note, query);
                if score > 0 {
                    matched_terms += 1;
                    total += score;
                }
            }
            if matched_terms == 0 || (match_all && matched_terms != queries.len()) {
                continue;
            }
            let reason = if matched_terms == 1 {
                format!("term match: {}", queries[0])
            } else {
                format!("term match ({matched_terms}/{} terms)", queries.len())
            };
            scored.push(self.hit(index, total, reason));
        }
        self.rank(scored, limit)
    }

    /// Traverse the wikilink graph outwards from `name` up to `depth` hops.
    pub fn get_linked_notes(
        &self,
        name: &str,
        depth: usize,
        direction: Direction,
        limit: usize,
    ) -> Result<Vec<Hit>> {
        let start = self.resolve_one(name)?;
        let depth = depth.max(1);
        let mut visited: BTreeSet<usize> = BTreeSet::new();
        visited.insert(start);
        let mut frontier: Vec<usize> = vec![start];
        let mut scores: BTreeMap<usize, (u32, String)> = BTreeMap::new();

        for hop in 1..=depth {
            let mut next: BTreeSet<usize> = BTreeSet::new();
            for index in &frontier {
                for (neighbour, kind) in self.neighbours(*index, direction) {
                    if neighbour == start || visited.contains(&neighbour) {
                        continue;
                    }
                    next.insert(neighbour);
                    let entry = scores.entry(neighbour).or_insert((0, String::new()));
                    entry.0 += (10 / hop as u32).max(1);
                    if entry.1.is_empty() {
                        entry.1 =
                            format!("{kind} of '{}' at hop {hop}", self.notes[*index].rel_path);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            visited.extend(next.iter().copied());
            frontier = next.into_iter().collect();
        }

        let hits = scores
            .into_iter()
            .map(|(index, (score, reason))| self.hit(index, score, reason));
        Ok(self.rank(hits, limit))
    }

    /// Resolved neighbours of a note in the requested direction(s).
    fn neighbours(&self, index: usize, direction: Direction) -> Vec<(usize, &'static str)> {
        let mut result = Vec::new();
        if matches!(direction, Direction::Outgoing | Direction::Both) {
            result.extend(self.outgoing[index].iter().map(|target| (*target, "link")));
        }
        if matches!(direction, Direction::Backlinks | Direction::Both) {
            result.extend(
                self.incoming[index]
                    .iter()
                    .map(|source| (*source, "backlink")),
            );
        }
        result.sort();
        result
    }

    /// Backlinks of a note, resolved to paths.
    pub fn backlink_paths(&self, note: &Note) -> Vec<String> {
        let Some(index) = self
            .notes
            .iter()
            .position(|other| other.rel_path == note.rel_path)
        else {
            return Vec::new();
        };
        self.incoming[index]
            .iter()
            .map(|source| self.notes[*source].rel_path.clone())
            .collect()
    }

    /// Outgoing links of a note that resolved to real files.
    pub fn resolved_link_paths(&self, note: &Note) -> Vec<String> {
        let Some(index) = self
            .notes
            .iter()
            .position(|other| other.rel_path == note.rel_path)
        else {
            return Vec::new();
        };
        self.outgoing[index]
            .iter()
            .map(|target| self.notes[*target].rel_path.clone())
            .collect()
    }

    fn hit(&self, index: usize, score: u32, reason: impl Into<String>) -> Hit {
        Hit {
            index,
            score,
            reason: reason.into(),
        }
    }

    /// Sort by score (desc) then path (asc) and truncate — deterministic.
    fn rank(&self, hits: impl IntoIterator<Item = Hit>, limit: usize) -> Vec<Hit> {
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

    // -- Rendering ----------------------------------------------------------

    /// Render search hits as an LLM-friendly markdown payload.
    pub fn render_hits(&self, hits: &[Hit], excerpt_chars: usize) -> String {
        if hits.is_empty() {
            return "No notes matched the query.".to_string();
        }
        let mut out = String::new();
        for hit in hits {
            let note = &self.notes[hit.index];
            let _ = writeln!(out, "## {}", note.title);
            let _ = writeln!(out, "- path: `{}`", note.rel_path);
            let _ = writeln!(out, "- score: {} ({})", hit.score, hit.reason);
            if !note.tags.is_empty() {
                let tags: Vec<String> = note.tags.iter().map(|tag| format!("#{tag}")).collect();
                let _ = writeln!(out, "- tags: {}", tags.join(", "));
            }
            let backlinks = self.backlink_paths(note);
            if !backlinks.is_empty() {
                let _ = writeln!(out, "- backlinks: {}", backlinks.join(", "));
            }
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
        if !note.tags.is_empty() {
            let tags: Vec<String> = note.tags.iter().map(|tag| format!("#{tag}")).collect();
            let _ = writeln!(out, "- tags: {}", tags.join(", "));
        }
        if !note.links.is_empty() {
            let links: Vec<String> = note
                .links
                .iter()
                .map(|link| format!("[[{link}]]"))
                .collect();
            let _ = writeln!(out, "- links: {}", links.join(", "));
        }
        let resolved = self.resolved_link_paths(note);
        if !resolved.is_empty() {
            let _ = writeln!(out, "- resolved links: {}", resolved.join(", "));
        }
        let backlinks = self.backlink_paths(note);
        if !backlinks.is_empty() {
            let _ = writeln!(out, "- backlinks: {}", backlinks.join(", "));
        }
        if !note.headings.is_empty() {
            let _ = writeln!(out, "- headings: {}", note.headings.join(" | "));
        }
        out.push('\n');
        out.push_str(&truncate_chars(note.body.trim(), body_chars));
        out
    }

    /// Deterministic one-line-per-note rendering of the whole index.
    pub fn render_index(&self, limit: usize) -> String {
        let stats = self.stats();
        let mut out = String::new();
        let _ = writeln!(out, "vault: {}", self.root.display());
        let _ = writeln!(
            out,
            "notes: {} | tags: {} | headings: {} | links: {} | unresolved links: {}",
            stats.notes, stats.tags, stats.headings, stats.links, stats.unresolved_links
        );
        if stats.notes > 0 {
            out.push('\n');
        }
        for note in self
            .notes
            .iter()
            .take(if limit == 0 { usize::MAX } else { limit })
        {
            let tags: Vec<String> = note.tags.iter().map(|tag| format!("#{tag}")).collect();
            let _ = writeln!(
                out,
                "{:<48} {:>3} links  {}",
                note.rel_path,
                note.links.len(),
                tags.join(" ")
            );
        }
        if limit > 0 && stats.notes > limit {
            let _ = writeln!(out, "… {} more notes", stats.notes - limit);
        }
        out.trim_end().to_string()
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse one markdown file into a [`Note`].
fn parse_note(rel_path: &str, raw: &str) -> Note {
    let (frontmatter, body) = split_frontmatter(raw);
    let title = frontmatter
        .get("title")
        .and_then(|values| values.first())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| file_stem(rel_path));

    let mut tags = BTreeSet::new();
    for key in TAG_KEYS {
        if let Some(values) = frontmatter.get(key) {
            for value in values {
                for part in split_list_value(value) {
                    let tag = normalize_tag(&part);
                    if !tag.is_empty() {
                        tags.insert(tag);
                    }
                }
            }
        }
    }

    let scan = scan_body(&body);
    tags.extend(scan.tags);

    Note {
        rel_path: rel_path.to_string(),
        title,
        tags,
        links: scan.links,
        headings: scan.headings,
        frontmatter,
        body,
    }
}

/// Split a `---` frontmatter block from the note body.
fn split_frontmatter(raw: &str) -> (BTreeMap<String, Vec<String>>, String) {
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let lines: Vec<&str> = text.lines().collect();

    let opens = lines
        .first()
        .map(|line| line.trim_end() == "---")
        .unwrap_or(false);
    if opens {
        let closing = lines
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, line)| matches!(line.trim_end(), "---" | "..."));
        if let Some((end, _)) = closing {
            let mut frontmatter = BTreeMap::new();
            parse_frontmatter(&lines[1..end], &mut frontmatter);
            let body = lines[end + 1..].join("\n");
            return (frontmatter, body);
        }
    }
    (BTreeMap::new(), text.to_string())
}

/// Flatten a YAML frontmatter block into `key -> values`.
///
/// Intentionally minimal: scalars, inline lists and `- item` lists are
/// supported; deeply nested YAML is ignored rather than guessed at.
fn parse_frontmatter(lines: &[&str], out: &mut BTreeMap<String, Vec<String>>) {
    let mut current: Option<String> = None;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if let Some(item) = trimmed.strip_prefix("- ") {
            if indented && let Some(key) = &current {
                out.entry(key.clone()).or_default().push(clean_scalar(item));
            }
            continue;
        }
        if indented {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        let value = value.trim();
        if !value.is_empty() {
            out.entry(key.clone())
                .or_default()
                .push(clean_scalar(value));
        } else {
            out.entry(key.clone()).or_default();
        }
        current = Some(key);
    }
}

/// Strip quotes from a frontmatter scalar.
fn clean_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_string()
}

/// Expand an inline list (`[a, b]`) or a single value into items.
fn split_list_value(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(clean_scalar)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Result of a single pass over a note body.
#[derive(Debug, Default)]
struct BodyScan {
    tags: BTreeSet<String>,
    links: BTreeSet<String>,
    headings: Vec<String>,
}

/// Collect inline `#tags`, `[[wikilinks]]` and headings, skipping code fences.
fn scan_body(body: &str) -> BodyScan {
    let mut scan = BodyScan::default();
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ') {
            let heading = trimmed[hashes..].trim().trim_end_matches('#').trim();
            if !heading.is_empty() {
                scan.headings.push(heading.to_string());
            }
        }
        scan.links.extend(extract_wikilinks(line));
        scan.tags.extend(extract_inline_tags(line));
    }
    scan
}

/// Extract wikilink targets from a line (`[[Target|alias]]` -> `Target`).
fn extract_wikilinks(line: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut offset = 0usize;
    while let Some(start) = line[offset..].find("[[") {
        let inner_start = offset + start + 2;
        let Some(end_rel) = line[inner_start..].find("]]") else {
            break;
        };
        let inner = &line[inner_start..inner_start + end_rel];
        let target = inner
            .split('|')
            .next()
            .unwrap_or_default()
            .split('#')
            .next()
            .unwrap_or_default()
            .trim();
        if !target.is_empty() {
            found.insert(target.to_string());
        }
        offset = inner_start + end_rel + 2;
        if offset >= line.len() {
            break;
        }
    }
    found
}

/// Extract Obsidian inline tags (`#project/alpha`) from a line.
fn extract_inline_tags(line: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '#' {
            let boundary = index == 0
                || chars[index - 1].is_whitespace()
                || matches!(chars[index - 1], '(' | '[' | '>' | '"' | '\'');
            if boundary {
                let mut cursor = index + 1;
                let mut tag = String::new();
                while cursor < chars.len()
                    && (chars[cursor].is_alphanumeric() || matches!(chars[cursor], '_' | '-' | '/'))
                {
                    tag.push(chars[cursor]);
                    cursor += 1;
                }
                let tag = normalize_tag(&tag);
                if tag.len() >= 2 {
                    found.insert(tag);
                }
                index = cursor.max(index + 1);
                continue;
            }
        }
        index += 1;
    }
    found
}

/// All names a note can be linked by: path, file stem and frontmatter aliases.
fn note_aliases(note: &Note) -> Vec<String> {
    let mut aliases = BTreeSet::new();
    let path_key = normalize_key(&note.rel_path);
    if !path_key.is_empty() {
        aliases.insert(path_key);
    }
    let stem_key = normalize_key(&file_stem(&note.rel_path));
    if !stem_key.is_empty() {
        aliases.insert(stem_key);
    }
    for key in ALIAS_KEYS {
        if let Some(values) = note.frontmatter.get(key) {
            for value in values {
                for part in split_list_value(value) {
                    let alias = normalize_key(&part);
                    if !alias.is_empty() {
                        aliases.insert(alias);
                    }
                }
            }
        }
    }
    aliases.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

/// Score how well `phrase` matches a note (0 = no match).
///
/// Every word must appear in at least one field, each word contributing only
/// its best field weight, so `"Beta project"` still matches a note titled
/// `Beta Note` tagged `project`. A whole-phrase hit adds a bonus so exact
/// matches outrank notes that merely contain the individual words.
fn score_terms(note: &Note, phrase: &str) -> u32 {
    let words: Vec<String> = phrase
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|word| !word.is_empty())
        .collect();
    if words.is_empty() {
        return 0;
    }

    let title = note.title.to_lowercase();
    let body = note.body.to_lowercase();
    let headings: Vec<String> = note
        .headings
        .iter()
        .map(|heading| heading.to_lowercase())
        .collect();
    let frontmatter: Vec<String> = note
        .frontmatter
        .values()
        .flatten()
        .map(|value| value.to_lowercase())
        .collect();

    let mut total = 0u32;
    for word in &words {
        total += if title.contains(word.as_str()) {
            5
        } else if note.tags.iter().any(|tag| tag.contains(word.as_str())) {
            4
        } else if headings
            .iter()
            .any(|heading| heading.contains(word.as_str()))
        {
            3
        } else if frontmatter
            .iter()
            .any(|value| value.contains(word.as_str()))
        {
            2
        } else if body.contains(word.as_str()) {
            1
        } else {
            return 0;
        };
    }

    if words.len() > 1 {
        let joined = words.join(" ");
        if title.contains(&joined)
            || body.contains(&joined)
            || headings.iter().any(|heading| heading.contains(&joined))
        {
            total += 3;
        }
    }
    total
}

/// Tokenise text into lowercased words of at least two characters.
fn collect_terms(text: &str, out: &mut BTreeSet<String>) {
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            if current.chars().count() >= 2 {
                out.insert(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.chars().count() >= 2 {
        out.insert(current);
    }
}

/// Normalise a tag: lowercase, no leading `#`, no surrounding whitespace.
fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('#')
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalise a note reference for alias lookups.
fn normalize_key(name: &str) -> String {
    let cleaned = name
        .split('|')
        .next()
        .unwrap_or_default()
        .split('#')
        .next()
        .unwrap_or_default()
        .trim()
        .replace('\\', "/")
        .to_lowercase();
    let cleaned = cleaned.trim_end_matches(".md").to_string();
    cleaned.trim_matches('/').to_string()
}

/// File stem without directories or extension.
fn file_stem(rel_path: &str) -> String {
    let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
    filename
        .strip_suffix(".md")
        .or_else(|| filename.strip_suffix(".MD"))
        .unwrap_or(filename)
        .to_string()
}

/// First `chars` characters of the body, collapsed to a readable excerpt.
fn excerpt_of(body: &str, chars: usize) -> String {
    let collapsed: String = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(&collapsed, chars)
}

/// Truncate to `chars` characters, appending an explicit marker.
fn truncate_chars(text: &str, chars: usize) -> String {
    if chars == 0 {
        return text.to_string();
    }
    let mut out = String::new();
    for (written, ch) in text.chars().enumerate() {
        if written >= chars {
            out.push_str(" …[truncated]");
            return out;
        }
        out.push(ch);
    }
    out
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

fn is_skipped_dir(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 || !entry.file_type().is_dir() {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|name| name.starts_with('.') || SKIPPED_DIRS.contains(&name))
        .unwrap_or(false)
}
