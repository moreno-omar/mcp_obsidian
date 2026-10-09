//! Terminal presentation layer: Tokyo Night output built on `richrs`.
//!
//! `main.rs` decides *what* is printed, this module decides *how*. Everything
//! writes through a [`Console`], which drops all colours when the stream is not
//! a terminal (or `NO_COLOR` is set). That keeps piped output plain text and
//! — critically for `serve` — keeps ANSI escapes out of the MCP JSON-RPC
//! stream, which runs on stdout.
//!
//! | submodule | contents |
//! | :- | :- |
//! | [`theme`] | the Tokyo Night palette, as [`Style`] values |
//! | [`markdown`] | markdown → styled segments, used for every model answer |
//! | `wrap` | line layout: word wrap for prose, hard wrap for code |

pub mod markdown;
pub mod theme;
mod wrap;

use richrs::console::{ColorSystem, Console};
use richrs::errors::Result as RichResult;
use richrs::panel::Panel;
use richrs::rule::Rule;
use richrs::style::Style;
use richrs::table::{Column, Table};
use richrs::text::Text;

use crate::vault::{Note, Vault, VaultStats};

/// Width used for rules, panels and tables; clamped so narrow terminals stay
/// readable and wide ones do not stretch forever.
const DEFAULT_WIDTH: usize = 80;

/// Console writing to stdout, plain text when stdout is redirected.
pub fn stdout_console() -> Console {
    plain_when_redirected(Console::new())
}

/// Console writing to stderr, plain text when stderr is redirected.
pub fn stderr_console() -> Console {
    plain_when_redirected(Console::stderr())
}

/// Disable colours unless the console is attached to a terminal.
fn plain_when_redirected(mut console: Console) -> Console {
    if std::env::var_os("NO_COLOR").is_some() {
        console.set_color_system(ColorSystem::None);
        return console;
    }
    if console.is_terminal() {
        return console;
    }
    if force_colours() {
        console.set_force_terminal(true);
        return console;
    }
    console.set_color_system(ColorSystem::None);
    console
}

/// `CLICOLOR_FORCE`/`FORCE_COLOR` request colours even when output is piped.
fn force_colours() -> bool {
    std::env::var_os("CLICOLOR_FORCE").is_some() || std::env::var_os("FORCE_COLOR").is_some()
}

/// Drawing width for the current terminal.
fn width_of(console: &Console) -> usize {
    let detected = console.width();
    if detected < 20 { DEFAULT_WIDTH } else { detected.min(100) }
}

/// Wrap width for prose: the terminal width, or `0` (no wrapping) when the
/// output is redirected. Piped text keeps its natural line structure, which is
/// what makes `ask … > answer.md` byte-faithful.
fn wrap_width(console: &Console) -> usize {
    if console.is_terminal() {
        width_of(console)
    } else {
        0
    }
}

/// Print a titled horizontal rule, the separator between answer sections.
pub fn print_rule(console: &mut Console, title: &str) -> RichResult<()> {
    let rule = Rule::with_title(title)
        .style(theme::border())
        .title_style(theme::blue().bold());
    console.write_segments(&rule.render(width_of(console)))?;
    console.write("\n")?;
    console.flush()
}

/// Render markdown, e.g. a model answer or a rendered vault payload.
pub fn print_markdown(console: &mut Console, source: &str) -> RichResult<()> {
    console.write_segments(&markdown::render(source, wrap_width(console)))?;
    console.flush()
}

/// Print a single styled line.
pub fn print_line(console: &mut Console, text: &str, style: Style) -> RichResult<()> {
    console.write_segments(&markdown::line(text, style))?;
    console.flush()
}

/// Print a `label : value` row, as used by `doctor` and the startup banners.
pub fn print_row(console: &mut Console, label: &str, value: &str, style: Style) -> RichResult<()> {
    let mut text = Text::new();
    text.append_plain("  ");
    text.append_styled(&format!("{label:<12}"), theme::label());
    text.append_styled(": ", theme::label());
    text.append_styled(value, style);
    console.print_text(&text)
}

/// Print a dimmed hint line.
pub fn print_hint(console: &mut Console, hint: &str) -> RichResult<()> {
    print_line(console, &format!("  {hint}"), theme::comment())
}

/// Print an error line.
pub fn print_error(console: &mut Console, message: &str) -> RichResult<()> {
    print_line(console, &format!("error: {message}"), theme::error())
}

/// Print a boxed panel with `label: value` rows, used for startup banners.
pub fn print_panel(
    console: &mut Console,
    title: &str,
    subtitle: &str,
    rows: &[(String, String)],
) -> RichResult<()> {
    let mut body = Text::new();
    for (index, (label, value)) in rows.iter().enumerate() {
        if index > 0 {
            body.append_plain("\n");
        }
        body.append_styled(&format!("{label:<10}"), theme::label());
        body.append_styled(value, theme::foreground());
    }
    let mut panel = Panel::new(body)
        .title(Text::styled(title, theme::title()))
        .border_style(theme::border())
        .title_style(theme::title());
    if !subtitle.is_empty() {
        panel = panel
            .subtitle(Text::styled(subtitle, theme::comment()))
            .subtitle_style(theme::comment());
    }
    console.write_segments(&panel.render(width_of(console)))?;
    console.flush()
}

/// Print the `index` command: vault statistics plus the note table.
pub fn print_index(console: &mut Console, vault: &Vault, limit: usize) -> RichResult<()> {
    let stats = vault.stats();
    print_row(
        console,
        "vault",
        &vault.root().display().to_string(),
        theme::path(),
    )?;
    print_row(console, "notes", &stats.notes.to_string(), theme::value())?;
    print_row(console, "tags", &stats.tags.to_string(), theme::value())?;
    print_row(console, "headings", &stats.headings.to_string(), theme::value())?;
    print_row(
        console,
        "links",
        &format!(
            "{} ({} unresolved)",
            stats.links, stats.unresolved_links
        ),
        theme::value(),
    )?;
    console.write("\n")?;

    let notes = limited_notes(vault, limit);
    if notes.is_empty() {
        print_hint(console, "no markdown notes found")?;
        return console.flush();
    }

    let mut table = Table::new()
        .header_style(theme::blue().bold())
        .border_style(theme::border());
    table.add_column(Column::new("path"));
    table.add_column(Column::new("links"));
    table.add_column(Column::new("tags"));
    for note in notes {
        table.add_row_cells([
            Text::styled(&note.rel_path, theme::path()),
            Text::styled(note.links.len().to_string(), theme::value()),
            Text::styled(tags_of(note).join(" "), theme::tag()),
        ]);
    }
    console.write_segments(&table.render(width_of(console)))?;
    console.write("\n")?;
    if limit > 0 && stats.notes > limit {
        print_hint(
            console,
            &format!("{} more notes (raise --limit to see them)", stats.notes - limit),
        )?;
    }
    console.flush()
}

/// Notes included in the index table, honouring `limit` (`0` = all).
fn limited_notes(vault: &Vault, limit: usize) -> &[Note] {
    let notes = vault.notes();
    if limit == 0 || limit >= notes.len() {
        notes
    } else {
        &notes[..limit]
    }
}

/// `#tag` values in the form Obsidian writes them.
fn tags_of(note: &Note) -> Vec<String> {
    note.tags.iter().map(|tag| format!("#{tag}")).collect()
}

/// Print the `search` command result: a query header plus the rendered hits.
pub fn print_search(console: &mut Console, kind: &str, query: &str, payload: &str) -> RichResult<()> {
    print_rule(console, &format!("{kind} · {query}"))?;
    print_markdown(console, payload)
}

/// Print the tool calls the model made, one dim line each.
pub fn print_trace(console: &mut Console, trace: &[(String, String)]) -> RichResult<()> {
    for (tool, arguments) in trace {
        let mut text = Text::new();
        text.append_styled("  · ", theme::trace());
        text.append_styled(tool, theme::trace_tool());
        text.append_styled(&format!(" {arguments}"), theme::trace());
        console.print_text(&text)?;
    }
    Ok(())
}

/// Print the deterministic pre-search line, so a grounded answer is explained.
pub fn print_prefetch(console: &mut Console, query: &str) -> RichResult<()> {
    let mut text = Text::new();
    text.append_styled("  · ", theme::trace());
    text.append_styled("prefetch", theme::trace_tool());
    text.append_styled(&format!(" search_by_term {query}"), theme::trace());
    console.print_text(&text)
}

/// Print the round/tool-call footer used by `--trace`.
pub fn print_summary(console: &mut Console, rounds: usize, tool_calls: usize) -> RichResult<()> {
    print_line(
        console,
        &format!("rounds: {rounds} | tool calls: {tool_calls}"),
        theme::trace(),
    )
}

/// Print the `ask` answer, rendered as markdown under a titled rule.
pub fn print_answer(console: &mut Console, answer: &str) -> RichResult<()> {
    print_markdown(console, answer)
}

/// Print a one-line description of the indexed vault.
pub fn print_vault_stats(console: &mut Console, stats: VaultStats, root: &str) -> RichResult<()> {
    print_row(console, "vault", root, theme::path())?;
    print_row(
        console,
        "indexed",
        &format!(
            "{} notes, {} tags, {} headings, {} links ({} unresolved)",
            stats.notes, stats.tags, stats.headings, stats.links, stats.unresolved_links
        ),
        theme::foreground(),
    )
}

/// Print the interactive `chat` welcome panel.
pub fn print_chat_banner(
    console: &mut Console,
    model: &str,
    url: &str,
    tools: &[String],
) -> RichResult<()> {
    let rows = vec![
        ("model".to_string(), model.to_string()),
        ("endpoint".to_string(), url.to_string()),
        (
            "tools".to_string(),
            if tools.is_empty() {
                "disabled".to_string()
            } else {
                tools.join(", ")
            },
        ),
        ("exit".to_string(), "/exit or Ctrl-D".to_string()),
    ];
    print_panel(console, "mcp-obsidian · chat", "", &rows)
}

/// Write the `you>` prompt without a trailing newline.
pub fn write_user_prompt(console: &mut Console) -> RichResult<()> {
    console.write_segments(&markdown::line("you> ", theme::user_prompt()))?;
    console.flush()
}

/// Print the `serve` startup banner on **stderr** (stdout carries JSON-RPC) for
/// either transport, so a host's stderr capture is the one place to look.
pub fn print_serve_banner(
    console: &mut Console,
    vault: &Vault,
    tools: &[String],
    transport: &str,
) -> RichResult<()> {
    let stats = vault.stats();
    let rows = vec![
        ("transport".to_string(), transport.to_string()),
        ("vault".to_string(), vault.root().display().to_string()),
        (
            "indexed".to_string(),
            format!("{} notes, {} tags, {} unresolved links", stats.notes, stats.tags, stats.unresolved_links),
        ),
        ("tools".to_string(), tools.join(", ")),
    ];
    print_panel(console, "mcp-obsidian · serve", "", &rows)
}
