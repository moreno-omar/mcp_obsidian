//! Line layout: styled runs in, wrapped physical lines out.
//!
//! [`super::markdown`] decides *what* a line says (block type, inline styles);
//! this module decides where it breaks. Two strategies live here:
//!
//! * [`push_wrapped_lines`] word-wraps prose, so a continuation line starts under
//!   the text rather than under the bullet or quote bar.
//! * [`push_code_lines`] hard-wraps code at cell boundaries (breaking at the last
//!   space when there is one) so its columns survive.
//!
//! Width `0` means "never wrap": redirected output keeps its natural lines.

use richrs::segment::{Segment, Segments};
use richrs::style::Style;

/// Narrowest column the renderer will wrap to.
pub(super) const MIN_WIDTH: usize = 20;

/// Gutter drawn to the left of a fenced code block.
const CODE_GUTTER: &str = "  │ ";

/// Cells the code gutter occupies, used to align wrapped code lines.
const CODE_INDENT: &str = "    ";

/// One stretch of text that shares a single style.
#[derive(Debug, Clone)]
pub(super) struct Run {
    pub(super) text: String,
    pub(super) style: Style,
}

impl Run {
    /// A styled run.
    pub(super) fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    /// Cells this run occupies on screen.
    fn width(&self) -> usize {
        cell_width(&self.text)
    }
}

/// A word plus everything the wrapper needs to place it.
#[derive(Debug, Clone)]
struct Word {
    text: String,
    style: Style,
    /// Whether a space belongs between this word and the previous one. False
    /// for pieces glued together by inline formatting, e.g. `` `code`, ``.
    space_before: bool,
}

/// Word-wrap `runs` and emit each physical line with its prefix.
pub(super) fn push_wrapped_lines(
    out: &mut Segments,
    runs: &[Run],
    first: &[Run],
    next: &[Run],
    width: usize,
) {
    for (index, physical) in word_wrap(runs, width, first, next).into_iter().enumerate() {
        push_runs(out, prefix_for(index, first, next));
        push_runs(out, &physical);
        out.push(Segment::newline());
    }
}

/// Emit one code-block line, gutter included, hard-wrapped when too long.
pub(super) fn push_code_lines(out: &mut Segments, line: &str, width: usize) {
    let first = plain(CODE_GUTTER);
    let next = plain(CODE_INDENT);
    let body = if line.is_empty() {
        Vec::new()
    } else {
        vec![Run::new(line, super::theme::code_block())]
    };
    for (index, chunk) in hard_wrap(&body, width, &first, &next).into_iter().enumerate() {
        push_runs(out, prefix_for(index, &first, &next));
        push_runs(out, &chunk);
        out.push(Segment::newline());
    }
}

/// Keep at most one blank line between blocks and none at the start.
///
/// Every rendered line ends in a newline, so two consecutive newlines *are* one
/// blank line; a third would be a double gap (a closing code fence followed by
/// a blank source line, for instance).
pub(super) fn collapse_blank_lines(segments: Segments) -> Segments {
    let mut out = Segments::new();
    let mut newlines = 0usize;
    for segment in segments {
        if is_newline(&segment) {
            newlines += 1;
            if out.is_empty() || newlines > 2 {
                continue;
            }
        } else {
            newlines = 0;
        }
        out.push(segment);
    }
    trim_trailing_blank(out)
}

/// Append one styled run, skipping empty text.
pub(super) fn push_run(out: &mut Segments, run: &Run) {
    if !run.text.is_empty() {
        out.push(Segment::styled(run.text.clone(), run.style.clone()));
    }
}

/// Append a list of styled runs.
pub(super) fn push_runs(out: &mut Segments, runs: &[Run]) {
    for run in runs {
        push_run(out, run);
    }
}

/// Unstyled runs for a literal prefix such as an indent or a bullet.
pub(super) fn plain(text: &str) -> Vec<Run> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Run::new(text, Style::default())]
    }
}

/// Display cells in `text`, using `richrs`' width rules (wide chars count 2).
fn cell_width(text: &str) -> usize {
    Segment::new(text).cell_length()
}

/// Greedy word wrap into physical lines, never returning an empty result.
///
/// `width == 0` disables wrapping. Words wider than a whole line are split at
/// cell boundaries, so even an unbreakable token makes progress.
fn word_wrap(runs: &[Run], width: usize, first: &[Run], next: &[Run]) -> Vec<Vec<Run>> {
    if width == 0 {
        return vec![runs.to_vec()];
    }
    let mut lines: Vec<Vec<Run>> = Vec::new();
    let mut current: Vec<Run> = Vec::new();
    let mut used = prefix_width(first);

    for word in words_of(runs) {
        let mut pending = word.text;
        while !pending.is_empty() {
            let separator = if current.is_empty() || !word.space_before {
                0
            } else {
                1
            };
            let free = width.saturating_sub(used + separator);
            if cell_width(&pending) <= free {
                if separator == 1 {
                    current.push(Run::new(" ", Style::default()));
                }
                used += separator + cell_width(&pending);
                current.push(Run::new(pending, word.style.clone()));
                pending = String::new();
                continue;
            }
            if current.is_empty() {
                let (head, tail) = split_cells(&pending, free.max(1));
                current.push(Run::new(head, word.style.clone()));
                pending = tail;
            } else if !word.space_before {
                // Glued to the previous word (`text,`): split it rather than
                // moving it to the next line, so the pair stays adjacent.
                let (head, tail) = split_cells(&pending, free.max(1));
                current.push(Run::new(head, word.style.clone()));
                pending = tail;
            }
            lines.push(std::mem::take(&mut current));
            used = prefix_width(next);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

/// Hard-wrap runs at cell boundaries, for text whose columns matter (code).
fn hard_wrap(runs: &[Run], width: usize, first: &[Run], next: &[Run]) -> Vec<Vec<Run>> {
    if width == 0 {
        return vec![runs.to_vec()];
    }
    let mut lines: Vec<Vec<Run>> = Vec::new();
    let mut current: Vec<Run> = Vec::new();
    let mut used = prefix_width(first);
    for run in runs {
        let mut pending = run.text.clone();
        while !pending.is_empty() {
            let free = width.saturating_sub(used);
            if cell_width(&pending) <= free {
                used += cell_width(&pending);
                current.push(Run::new(pending, run.style.clone()));
                pending = String::new();
                continue;
            }
            if current.is_empty() {
                let (head, tail) = split_cells(&pending, free.max(1));
                let (head, tail) = break_at_space(head, tail);
                current.push(Run::new(head, run.style.clone()));
                pending = tail;
            }
            lines.push(std::mem::take(&mut current));
            used = prefix_width(next);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

/// Split `text` after `cells` display columns; always keeps at least one column
/// on the left so callers cannot loop forever.
fn split_cells(text: &str, cells: usize) -> (String, String) {
    let (left, right) = Segment::new(text).split_at(cells.max(1));
    (left.to_string(), right.to_string())
}

/// Move the tail of `head` behind its last space onto the continuation line, so
/// hard-wrapped code breaks between tokens whenever there is a space to use.
fn break_at_space(head: String, tail: String) -> (String, String) {
    match head.rfind(' ') {
        Some(index) if index > 0 => {
            let moved = &head[index + 1..];
            (head[..index].to_string(), format!("{moved}{tail}"))
        }
        _ => (head, tail),
    }
}

/// Flatten runs into words, remembering each word's style and whether a space
/// precedes it. Runs that inline formatting glued together (`` `code`, `` or
/// `**bold**!`) stay glued, so the wrapper never invents or loses a space.
fn words_of(runs: &[Run]) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut space_pending = false;
    for run in runs {
        let pieces: Vec<&str> = run.text.split_whitespace().collect();
        if pieces.is_empty() {
            space_pending = space_pending || !run.text.is_empty();
            continue;
        }
        let opens_with_space = run.text.starts_with(char::is_whitespace);
        for (index, piece) in pieces.iter().enumerate() {
            words.push(Word {
                text: (*piece).to_string(),
                style: run.style.clone(),
                space_before: index > 0 || space_pending || opens_with_space,
            });
        }
        space_pending = run.text.ends_with(char::is_whitespace);
    }
    words
}

/// The prefix for the `index`-th physical line of a wrapped block.
fn prefix_for<'a>(index: usize, first: &'a [Run], next: &'a [Run]) -> &'a [Run] {
    if index == 0 { first } else { next }
}

/// Total cells taken by a prefix.
fn prefix_width(prefix: &[Run]) -> usize {
    prefix.iter().map(Run::width).sum()
}

/// Drop trailing blank lines, keeping the single terminating newline.
fn trim_trailing_blank(segments: Segments) -> Segments {
    let mut collected: Vec<Segment> = segments.into_iter().collect();
    while collected.len() >= 2 {
        let last_is_newline = is_newline(&collected[collected.len() - 1]);
        let before_is_newline = is_newline(&collected[collected.len() - 2]);
        if last_is_newline && before_is_newline {
            collected.pop();
        } else {
            break;
        }
    }
    collected.into_iter().collect()
}

/// Whether a segment is a bare newline.
fn is_newline(segment: &Segment) -> bool {
    segment.to_string() == "\n"
}
