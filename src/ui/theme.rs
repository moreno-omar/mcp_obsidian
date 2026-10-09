//! Tokyo Night palette mapped onto [`richrs`] styles.
//!
//! Every colour the terminal UI uses lives here, so the look of the CLI can be
//! changed in one place. The values are the true colour RGB codes of the Tokyo
//! Night theme, which reads well on both dark and light terminals.

use richrs::color::Color;
use richrs::style::Style;

/// Build a true colour value from its components.
const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

/// Primary foreground colour, used for body text and answers.
pub fn foreground() -> Style {
    Style::new().with_color(rgb(0xc0, 0xca, 0xf5))
}

/// Muted foreground colour, used inside fenced code blocks.
pub fn foreground_dark() -> Style {
    Style::new().with_color(rgb(0xa9, 0xb1, 0xd6))
}

/// Comment grey, used for hints, borders and secondary metadata.
pub fn comment() -> Style {
    Style::new().with_color(rgb(0x56, 0x5f, 0x89))
}

/// Blue accent, used for headings and the assistant prompt.
pub fn blue() -> Style {
    Style::new().with_color(rgb(0x7a, 0xa2, 0xf7))
}

/// Cyan accent, used for bullets, links and inline emphasis.
pub fn cyan() -> Style {
    Style::new().with_color(rgb(0x7d, 0xcf, 0xff))
}

/// Teal accent, used for the user prompt.
pub fn teal() -> Style {
    Style::new().with_color(rgb(0x73, 0xda, 0xca))
}

/// Green, used for successful states.
pub fn green() -> Style {
    Style::new().with_color(rgb(0x9e, 0xce, 0x6a))
}

/// Yellow, used for inline code and warnings.
pub fn yellow() -> Style {
    Style::new().with_color(rgb(0xe0, 0xaf, 0x68))
}

/// Orange, used for highlighted values.
pub fn orange() -> Style {
    Style::new().with_color(rgb(0xff, 0x9e, 0x64))
}

/// Red, used for errors.
pub fn red() -> Style {
    Style::new().with_color(rgb(0xf7, 0x76, 0x8e))
}

/// Purple, used for secondary headings and quotes.
pub fn purple() -> Style {
    Style::new().with_color(rgb(0xbb, 0x9a, 0xf7))
}

/// Magenta, used for `#tag` values.
pub fn magenta() -> Style {
    Style::new().with_color(rgb(0xff, 0x00, 0x7c))
}

/// Style for a markdown heading of the given level (1-6).
pub fn heading(level: usize) -> Style {
    let accent = match level {
        1 => blue(),
        2 => purple(),
        3 => cyan(),
        _ => teal(),
    };
    accent.bold()
}

/// Style for note paths and other identifiers the user may copy.
pub fn path() -> Style {
    cyan()
}

/// Style for note titles.
pub fn title() -> Style {
    blue().bold()
}

/// Style for `#tag` values.
pub fn tag() -> Style {
    magenta()
}

/// Style for inline `code` spans.
pub fn inline_code() -> Style {
    yellow()
}

/// Style for fenced code block bodies.
pub fn code_block() -> Style {
    foreground_dark()
}

/// Style for the `│` gutter marking a fenced code block.
pub fn code_gutter() -> Style {
    comment()
}

/// Style for markdown block quotes.
pub fn quote() -> Style {
    comment().italic()
}

/// Style for horizontal rules, table borders and panels.
pub fn border() -> Style {
    comment()
}

/// Style for the tool-call trace lines.
pub fn trace() -> Style {
    comment()
}

/// Style for the tool name inside a trace line.
pub fn trace_tool() -> Style {
    cyan()
}

/// Style for the local CLI prompt (`you>`).
pub fn user_prompt() -> Style {
    teal().bold()
}

/// Style for the model prompt (`llm>`).
pub fn assistant_prompt() -> Style {
    blue().bold()
}

/// Style for successful status values.
pub fn success() -> Style {
    green().bold()
}

/// Style for warning status values.
pub fn warn() -> Style {
    yellow().bold()
}

/// Style for failed status values.
pub fn error() -> Style {
    red().bold()
}

/// Style for labels in tables and key/value blocks.
pub fn label() -> Style {
    comment()
}

/// Style for values in tables and key/value blocks.
pub fn value() -> Style {
    orange()
}
