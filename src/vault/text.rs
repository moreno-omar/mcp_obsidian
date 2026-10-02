//! Small, pure string helpers shared by parsing, scoring and rendering.
//!
//! Everything here is a free function over `&str`/`&Note`: no vault state, no
//! I/O, so each one is trivially readable and independently testable.

use std::collections::BTreeSet;

use super::note::Note;

/// Score how well `phrase` matches a note (0 = no match).
///
/// Every word must appear in at least one field, each word contributing only
/// its best field weight, so `"Beta project"` still matches a note titled
/// `Beta Note` tagged `project`. A whole-phrase hit adds a bonus so exact
/// matches outrank notes that merely contain the individual words.
pub(crate) fn score_terms(note: &Note, phrase: &str) -> u32 {
    let words = words_of(phrase);
    if words.is_empty() {
        return 0;
    }

    let title = note.title.to_lowercase();
    let body = note.body.to_lowercase();
    let headings = lowercase_all(note.headings.iter());
    let frontmatter = lowercase_all(note.frontmatter.values().flatten());

    let mut total = 0u32;
    for word in &words {
        total += if title.contains(word.as_str()) {
            5
        } else if note.tags.iter().any(|tag| tag.contains(word.as_str())) {
            4
        } else if headings.iter().any(|heading| heading.contains(word.as_str())) {
            3
        } else if frontmatter.iter().any(|value| value.contains(word.as_str())) {
            2
        } else if body.contains(word.as_str()) {
            1
        } else {
            return 0;
        };
    }

    if words.len() > 1 && has_phrase_hit(&words.join(" "), &title, &body, &headings) {
        total += 3;
    }
    total
}

/// Split a query phrase into its non-empty lowercased words.
pub(crate) fn words_of(phrase: &str) -> Vec<String> {
    phrase
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|word| !word.is_empty())
        .collect()
}

/// Tokenise text into lowercased words of at least two characters.
///
/// The length floor keeps the inverted index free of noise words such as `a`
/// or `of`. `_` and `-` count as word characters so `beta-note` stays whole.
pub(crate) fn collect_terms(text: &str, out: &mut BTreeSet<String>) {
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            current.extend(ch.to_lowercase());
        } else {
            flush_term(&mut current, out);
        }
    }
    flush_term(&mut current, out);
}

/// Normalise a tag: lowercase, no leading `#`, no surrounding whitespace.
/// Inner whitespace becomes `-` so `#My Tag` and `#my-tag` are one tag.
pub(crate) fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('#')
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalise a note reference for alias lookups.
///
/// Handles every way a note is addressed in practice: wikilink aliases and
/// headings (`note#section`, `note|label`) are dropped, backslashes become
/// slashes, the `.md` suffix is removed, surrounding slashes are trimmed and
/// the result is lowercased.
pub(crate) fn normalize_key(name: &str) -> String {
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
pub(crate) fn file_stem(rel_path: &str) -> String {
    let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
    filename
        .strip_suffix(".md")
        .or_else(|| filename.strip_suffix(".MD"))
        .unwrap_or(filename)
        .to_string()
}

/// First `chars` characters of the body, collapsed to a readable excerpt.
pub(crate) fn excerpt_of(body: &str, chars: usize) -> String {
    let collapsed: String = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(&collapsed, chars)
}

/// Truncate to `chars` characters, appending an explicit marker.
/// `chars == 0` means "no budget", i.e. return the text unchanged.
pub(crate) fn truncate_chars(text: &str, chars: usize) -> String {
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

/// Push a buffered token when it is long enough to be worth indexing.
fn flush_term(current: &mut String, out: &mut BTreeSet<String>) {
    if current.chars().count() >= 2 {
        out.insert(std::mem::take(current));
    } else {
        current.clear();
    }
}

/// Does the joined phrase appear verbatim in a strong field?
fn has_phrase_hit(joined: &str, title: &str, body: &str, headings: &[String]) -> bool {
    title.contains(joined) || body.contains(joined) || headings.iter().any(|h| h.contains(joined))
}

/// Lowercase every item, keeping order.
fn lowercase_all<'a>(items: impl Iterator<Item = &'a String>) -> Vec<String> {
    items.map(|item| item.to_lowercase()).collect()
}
