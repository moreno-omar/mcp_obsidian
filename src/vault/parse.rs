//! Markdown → [`Note`]: frontmatter, headings, wikilinks and tags.
//!
//! The parser is deliberately shallow and dependency-free: scalars, inline
//! lists and `- item` lists are understood, deeply nested YAML is ignored
//! rather than guessed at (see "Possible problems" in `SUMMARY.md`).

use std::collections::{BTreeMap, BTreeSet};

use super::note::Note;
use super::text::{file_stem, normalize_key, normalize_tag};

/// Frontmatter keys that hold tags.
const TAG_KEYS: [&str; 2] = ["tags", "tag"];
/// Frontmatter keys that hold alternative note names.
const ALIAS_KEYS: [&str; 2] = ["aliases", "alias"];

/// Result of a single pass over a note body.
#[derive(Debug, Default)]
struct BodyScan {
    tags: BTreeSet<String>,
    links: BTreeSet<String>,
    headings: Vec<String>,
}

/// Parse one markdown file into a [`Note`].
pub(crate) fn parse_note(rel_path: &str, raw: &str) -> Note {
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

/// All names a note can be linked by: path, file stem and frontmatter aliases.
pub(crate) fn note_aliases(note: &Note) -> Vec<String> {
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
// Frontmatter
// ---------------------------------------------------------------------------

/// Split a `---` frontmatter block from the note body.
///
/// A UTF-8 BOM is tolerated. Without a leading `---` line (or without a closing
/// `---` / `...`), the whole file is treated as body.
fn split_frontmatter(raw: &str) -> (BTreeMap<String, Vec<String>>, String) {
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let lines: Vec<&str> = text.lines().collect();

    let opens = lines
        .first()
        .map(|line| line.trim_end() == "---")
        .unwrap_or(false);
    if opens
        && let Some((end, _)) = lines
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, line)| matches!(line.trim_end(), "---" | "..."))
    {
        let mut frontmatter = BTreeMap::new();
        parse_frontmatter(&lines[1..end], &mut frontmatter);
        let body = lines[end + 1..].join("\n");
        return (frontmatter, body);
    }
    (BTreeMap::new(), text.to_string())
}

/// Flatten a YAML frontmatter block into `key -> values`.
///
/// Only what an Obsidian vault actually needs: `key: value`, `key: [a, b]` and
/// indented `- item` entries continuing the previous key. Indented lines that
/// are neither are ignored.
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
        let entry = out.entry(key.clone()).or_default();
        let value = value.trim();
        if !value.is_empty() {
            entry.push(clean_scalar(value));
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

// ---------------------------------------------------------------------------
// Body scanning
// ---------------------------------------------------------------------------

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
        if let Some(heading) = heading_text(trimmed) {
            scan.headings.push(heading);
        }
        scan.links.extend(extract_wikilinks(line));
        scan.tags.extend(extract_inline_tags(line));
    }
    scan
}

/// Heading text for an ATX heading line (`## Title`), if any.
fn heading_text(trimmed_line: &str) -> Option<String> {
    let hashes = trimmed_line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) || trimmed_line.chars().nth(hashes) != Some(' ') {
        return None;
    }
    let heading = trimmed_line[hashes..].trim().trim_end_matches('#').trim();
    (!heading.is_empty()).then(|| heading.to_string())
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
///
/// A `#` only starts a tag at the beginning of a line or after whitespace or
/// punctuation, so URLs and `C#` are not mistaken for tags.
fn extract_inline_tags(line: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '#' && starts_tag(&chars, index) {
            let mut cursor = index + 1;
            let mut tag = String::new();
            while cursor < chars.len() && is_tag_char(chars[cursor]) {
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
        index += 1;
    }
    found
}

/// Is the `#` at `index` a tag opener rather than part of a word or URL?
fn starts_tag(chars: &[char], index: usize) -> bool {
    index == 0
        || chars[index - 1].is_whitespace()
        || matches!(chars[index - 1], '(' | '[' | '>' | '"' | '\'')
}

/// Characters allowed inside an inline tag.
fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/')
}
