//! Markdown → styled terminal output.
//!
//! Model answers and vault excerpts are markdown. `richrs` ships its own
//! markdown renderer, but it only emits attributes (bold/dim), never theme
//! colours, so this module walks the source line by line and applies the Tokyo
//! Night palette from [`super::theme`].
//!
//! Supported subset — the one an LLM actually produces: headings, bullet and
//! numbered lists, block quotes, fenced code, horizontal rules, plus the inline
//! forms `` `code` ``, `**bold**`, `*italic*`/`_italic_` and `[label](url)`.
//!
//! This module owns *what* a line says; the private `wrap` module owns where it
//! breaks.

use richrs::segment::{Segment, Segments};
use richrs::style::Style;

use super::theme;
use super::wrap::{self, Run};

/// Characters that may open an inline styled run.
const MARKERS: [&str; 5] = ["`", "**", "*", "_", "["];

/// Width of a rendered `---` rule.
const RULE_WIDTH: usize = 40;

/// Largest list indentation (in levels) that is reproduced.
const MAX_INDENT_LEVELS: usize = 4;

/// Render markdown `source`, wrapping to `width` columns (`0` = never wrap).
pub fn render(source: &str, width: usize) -> Segments {
    let width = normalise_width(width);
    let mut out = Segments::new();
    let mut in_code = false;
    for line in source.lines() {
        if is_fence(line) {
            in_code = !in_code;
            if !in_code {
                out.push(Segment::newline());
            }
            continue;
        }
        if in_code {
            wrap::push_code_lines(&mut out, line, width);
            continue;
        }
        push_block_line(&mut out, line, width);
    }
    wrap::collapse_blank_lines(out)
}

/// A single styled line, ending in a newline.
pub fn line(text: &str, style: Style) -> Segments {
    let mut out = Segments::new();
    wrap::push_run(&mut out, &Run::new(text, style));
    out.push(Segment::newline());
    out
}

/// `0` keeps "no wrapping"; anything narrower than the minimum is raised.
fn normalise_width(width: usize) -> usize {
    if width == 0 {
        0
    } else {
        width.max(wrap::MIN_WIDTH)
    }
}

/// Render one non-code line: heading, rule, quote, list item or paragraph.
fn push_block_line(out: &mut Segments, line: &str, width: usize) {
    let content = line.trim_end();
    if content.trim().is_empty() {
        out.push(Segment::newline());
        return;
    }
    let indent = indent_prefix(line);
    let body = content.trim_start();

    if let Some((level, rest)) = split_heading(body) {
        let prefix = wrap::plain(&indent);
        let runs = inline_runs(rest, theme::heading(level));
        wrap::push_wrapped_lines(out, &runs, &prefix, &prefix, width);
        return;
    }
    if is_rule(body) {
        let rule_width = if width == 0 {
            RULE_WIDTH
        } else {
            RULE_WIDTH.min(width)
        };
        wrap::push_run(out, &Run::new("─".repeat(rule_width), theme::border()));
        out.push(Segment::newline());
        return;
    }
    if let Some(rest) = strip_quote(body) {
        let first = vec![Run::new(format!("{indent}▌ "), theme::purple())];
        let next = wrap::plain(&format!("{indent}  "));
        let runs = inline_runs(rest, theme::quote());
        wrap::push_wrapped_lines(out, &runs, &first, &next, width);
        return;
    }
    if let Some((marker, rest)) = split_list_item(body) {
        let first = vec![Run::new(format!("{indent}{marker}"), theme::cyan())];
        let next = wrap::plain(&format!("{indent}{}", " ".repeat(cell_width(&marker))));
        let runs = inline_runs(rest, theme::foreground());
        wrap::push_wrapped_lines(out, &runs, &first, &next, width);
        return;
    }
    let prefix = wrap::plain(&indent);
    let runs = inline_runs(body, theme::foreground());
    wrap::push_wrapped_lines(out, &runs, &prefix, &prefix, width);
}

/// Parse the inline forms of `source` into styled runs.
fn inline_runs(source: &str, base: Style) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut rest = source;
    while !rest.is_empty() {
        if let Some((label, url, remainder)) = split_link(rest) {
            runs.push(Run::new(label, theme::blue().underline()));
            if !label.eq_ignore_ascii_case(url) {
                runs.push(Run::new(format!(" ({url})"), theme::comment()));
            }
            rest = remainder;
            continue;
        }
        if let Some((value, style, remainder)) = take_inline_token(rest) {
            runs.push(Run::new(value, style));
            rest = remainder;
            continue;
        }
        let (plain, remainder) = take_plain(rest);
        runs.push(Run::new(plain, base.clone()));
        rest = remainder;
    }
    runs
}

/// Try every inline form, longest marker first.
fn take_inline_token(source: &str) -> Option<(String, Style, &str)> {
    inline_code(source)
        .or_else(|| inline_strong(source))
        .or_else(|| inline_emphasis(source))
}

/// `` `code` `` → yellow inline code.
fn inline_code(source: &str) -> Option<(String, Style, &str)> {
    let body = source.strip_prefix('`')?;
    let end = body.find('`')?;
    let (code, tail) = body.split_at(end);
    if code.trim().is_empty() {
        return None;
    }
    Some((code.to_string(), theme::inline_code(), skip(tail, 1)))
}

/// `**bold**` → bold foreground.
fn inline_strong(source: &str) -> Option<(String, Style, &str)> {
    let body = source.strip_prefix("**")?;
    let end = body.find("**")?;
    let (inner, tail) = body.split_at(end);
    if inner.trim().is_empty() {
        return None;
    }
    Some((inner.to_string(), theme::foreground().bold(), skip(tail, 2)))
}

/// `*italic*` or `_italic_` → italic foreground.
fn inline_emphasis(source: &str) -> Option<(String, Style, &str)> {
    let marker = source.chars().next()?;
    if marker != '*' && marker != '_' {
        return None;
    }
    let body = skip(source, marker.len_utf8());
    if body.starts_with(char::is_whitespace) {
        return None;
    }
    let end = body.find(marker)?;
    let inner = body.get(..end)?;
    if inner.is_empty() || inner.ends_with(char::is_whitespace) {
        return None;
    }
    Some((
        inner.to_string(),
        theme::foreground().italic(),
        skip(body, end + 1),
    ))
}

/// `[label](url)` → underlined label plus the url when it adds information.
fn split_link(source: &str) -> Option<(&str, &str, &str)> {
    let body = source.strip_prefix('[')?;
    let label_end = body.find(']')?;
    let label = body.get(..label_end)?;
    let tail = body.get(label_end..)?.strip_prefix("](")?;
    let url_end = tail.find(')')?;
    let url = tail.get(..url_end)?;
    Some((label, url, skip(tail, url_end + 1)))
}

/// Longest plain prefix of `source` that contains no inline marker.
///
/// Always consumes at least one character, so a lone `*` or `[` cannot stall
/// the inline scanner.
fn take_plain(source: &str) -> (&str, &str) {
    let start = source.chars().next().map(char::len_utf8).unwrap_or(0);
    let mut cut = source.len();
    for marker in MARKERS {
        if let Some(offset) = skip(source, start).find(marker) {
            cut = cut.min(start + offset);
        }
    }
    source.split_at(cut)
}

/// `line[offset..]`, tolerating an offset past the end.
fn skip(line: &str, offset: usize) -> &str {
    line.get(offset..).unwrap_or("")
}

/// Split `# Heading` into its level and text.
fn split_heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = skip(line, hashes).strip_prefix(' ')?;
    Some((hashes, rest.trim_end()))
}

/// `---`, `***` or `___` on their own line.
fn is_rule(line: &str) -> bool {
    let mut dashes = 0usize;
    for ch in line.chars() {
        match ch {
            '-' | '*' | '_' => dashes += 1,
            ' ' => {}
            _ => return false,
        }
    }
    dashes >= 3
}

/// `> quoted text` → the text without its marker.
fn strip_quote(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// `- item`, `* item`, `+ item` or `1. item` → bullet marker and text.
fn split_list_item(line: &str) -> Option<(String, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some(("• ".to_string(), rest));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = skip(line, digits).strip_prefix(". ")?;
    Some((format!("{}. ", &line[..digits]), rest))
}

/// Opening or closing fence of a fenced code block.
fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Reproduce list/quote indentation as two spaces per level, capped.
fn indent_prefix(line: &str) -> String {
    let spaces = line.chars().take_while(|ch| *ch == ' ').count();
    let levels = (spaces / 2).min(MAX_INDENT_LEVELS);
    "  ".repeat(levels)
}

/// Display cells in `text`, using `richrs`' width rules (wide chars count 2).
fn cell_width(text: &str) -> usize {
    Segment::new(text).cell_length()
}
