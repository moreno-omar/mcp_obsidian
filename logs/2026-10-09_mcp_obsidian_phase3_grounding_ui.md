# 2026-10-09 — mcp_obsidian Phase 3 (grounding + terminal UI)

## 14:05 — Phase 3: make `ask` use the vault, and render everything with richrs

### Objective

Implement every unchecked item in `TASKS.md` (Phase 3 "Improve response" of
`ROADMAP.md`), whose exit criterion is *"can answer 'how does one do list
comprehensions in python?' based on note in vault"*:

1. diagnose why the app fails to use the tool when the command `ask` is used;
2. improve `ask` output using `richrs` with a Tokyo Night theme;
3. improve `chat` output the same way;
4. improve the output when used as an MCP server.

The reported failure (`failed_command.md`), against a 490-note vault that
contains `python/python_oneliners/list comprehensions.md`:

```
rounds: 1 | tool calls: 0
I apologize, but I cannot answer that question. My capabilities are limited to
searching and retrieving information from the provided Obsidian markdown vault …
```

### Solution

**Root cause (measured, not guessed).** The request was fine; the *model's
decision* was not. The same prompt and tool definitions were sent straight to
`POST /v1/chat/completions` and the round-1 behaviour was counted:

| configuration | tool call in round 1 |
| :- | :- |
| old system prompt | 1/3 |
| imperative "search before answering" prompt | 4/6 |
| `tool_choice: "required"` | 1/3 — Ollama's OpenAI endpoint ignores it |
| tool schemas without `$schema`/`format`/`default` | 2/4 |
| `temperature: 0` | 0/4 — deterministic, answers directly |

Two complementary fixes:

1. **Imperative protocol** in `SERVER_INSTRUCTIONS` (shared by the host system
   prompt and the MCP `instructions` field) and in all four tool descriptions;
   `search_by_term` now tells the model to pass subject words as separate terms.
2. **Deterministic pre-search** (`Host::prefetch_notes`): before any model call,
   the prompt's stopword-filtered words (≤ 8) are run through the existing
   retrieval core and the top 3 hits are injected into the system prompt. It
   costs milliseconds, needs no model, and guarantees the relevant notes are in
   context even when the model never calls a tool. `--no-prefetch` disables it.

**UI.** New module tree `src/ui/` built on `richrs` 0.2.1:

* `ui/theme.rs` — Tokyo Night RGB palette as semantic `Style` helpers;
* `ui/markdown.rs` — markdown → styled segments (headings, bullets, numbered
  lists, quotes, fenced code, rules, inline code/bold/italic/links);
* `ui/mod.rs` — tty/`NO_COLOR`/`FORCE_COLOR` aware `Console`s, rules, panels,
  tables and the per-command views.

Wired into `ask`, `chat`, `doctor`, `index` (now a `richrs::Table`), `search` and
`serve`. `serve` prints its banner to **stderr** only, so the JSON-RPC stream on
stdout stays byte-clean; `serve -vv` logs each MCP tool call. A closed stdout
(`cmd | head`) is treated as success instead of an I/O error.

### Actions Performed

1. `cargo build --release` — reproduced the failure: 3 of 5 runs of
   `ask "how does one do list comprehensions in python?" --trace` reported
   `rounds: 1 | tool calls: 0` plus a refusal.
2. Wrote `/tmp/tool_probe*.py` — variants of the same request to
   `/v1/chat/completions` and `/api/chat`, varying system prompt, `tool_choice`,
   schema shape, `temperature`, `seed` and `think`. Results in the table above.
3. Read the raw response body: with thinking enabled the reply carries a
   `reasoning` field *and* a `tool_calls` array; the failing runs returned text
   content only, i.e. the model chose to answer.
4. Added `src/ui/{mod,theme,markdown}.rs` and `pub mod ui` in `src/lib.rs`.
5. `src/host.rs`: imperative prompt wiring, `prefetch_notes` + `search_terms` +
   stopword filter, `AskOutcome.prefetch`, `without_prefetch()`, UI-rendered
   `chat_loop`.
6. `src/mcp_server.rs`: strengthened `SERVER_INSTRUCTIONS`, tool descriptions and
   the MCP `instructions` field; added `log_tool_call` debug logging.
7. `src/main.rs`: UI for every command, `--no-prefetch` on `ask`/`chat`,
   `closed_stdout`/`is_broken_pipe` handling, `run(&cli)` dispatch split.
8. `cargo clippy --release --all-targets` (clean), `cargo test --release`
   (22 + 10 passed, 1 ignored), manual checks of every subcommand and of an MCP
   stdio session.

### Verification Output

Phase 3 exit criterion, 5 consecutive runs over `~/rust/test_vault` (490 notes):

```
$ ./target/release/mcp_obsidian --vault ~/rust/test_vault \
    ask "how does one do list comprehensions in python?" --trace
  · prefetch search_by_term [list, comprehensions, python]
  · get_note {"note":"python/python_oneliners/list comprehensions.md"}
rounds: 2 | tool calls: 1
  … "According to the note python/python_oneliners/list comprehensions.md, the
     general pattern is: [ expression + context + conditional ]"
```

Before: 3/5 runs refused with `rounds: 1 | tool calls: 0`.
After: 0/5 refusals, 5/5 grounded, 8/8 runs (including `--no-prefetch`) called at
least one tool. One run answered correctly from the pre-searched notes without
any tool call — the safety net working. A final tweak (prompt the model to call
`get_note` when an excerpt ends in `…[truncated]`, `PREFETCH_CHARS` 600 → 800)
gave 3/3 runs with a tool call and a 340–460 word, path-citing answer.

`--no-tools` remains a plain round trip: `ask "reply with the single word: pong"
--no-tools` → `rounds: 1 | tool calls: 0` / `pong` (no pre-search injected).

MCP server (banner on stderr, pure JSON-RPC on stdout):

```
╭─ mcp_obsidian · serve ───────────────────────────────────────────────────────╮
│ transport stdio (stdout = JSON-RPC)                                          │
│ vault     /home/redram/rust/mcp_obsidian/samples/vault                       │
│ indexed   4 notes, 11 tags, 0 unresolved links                               │
│ tools     search_by_tag, search_by_term, get_linked_notes, get_note          │
╰──────────────────────────────────────────────────────────────────────────────╯
$ … serve -vv  →  DEBUG mcp tool call tool="get_note" args=GetNoteRequest { note: "index" }
```

Tests and lints:

```
tests/vault_queries.rs   test result: ok. 22 passed; 0 failed
tests/ollama_client.rs   test result: ok. 10 passed; 0 failed; 1 ignored
cargo clippy --release --all-targets → no warnings
```

### References Used

1. Ollama — *Tool calling* (tool definitions, `tool_choice`, `think`):
   <https://docs.ollama.com/capabilities/tool-calling>
2. `richrs` 0.2.1 API documentation (Console, Style, Markdown, Table, Panel):
   <https://docs.rs/richrs/0.2.1/richrs/>
3. Model Context Protocol — *Lifecycle* (`initialize`, server `instructions`):
   <https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle>

### Optional steps

* Add a `--json` flag to `ask` for machine-readable answers plus the note paths
  used, so scripts do not have to parse rendered markdown.
* Unit-test `ui/markdown.rs` with `Console` in record mode — no terminal needed,
  and the renderer is now the most string-heavy code in the crate.
* Ship a `man`-style `--help` example block, since `ask` now has two behaviour
  switches (`--no-tools`, `--no-prefetch`) that change grounding.
* Wire the pre-search into the MCP path as an optional `resources` subscription
  hint, so external hosts get the same safety net without a tool round trip.

### Possible problems

* The pre-search is lexical. Prompt words that are common in the vault inject a
  semi-random slice of notes, and a small model may anchor on it; `--no-prefetch`
  is the escape hatch, and the constants (`PREFETCH_NOTES`, `PREFETCH_CHARS`) are
  deliberately small.
* Grounding is not verification: nothing yet checks that the answer cites a path
  the tools actually returned. A hallucinated citation would look authoritative.
* The hand-rolled markdown renderer ignores tables and `$$…$$` formulas, which
  the model emits; they currently print as raw text.
* Long lines are left to the terminal to wrap, so a wrapped code-block line loses
  the `  │ ` gutter alignment.
* Adding `src/ui/` makes `richrs` a functional dependency of every command; if
  `richrs` regresses, `ask`/`chat` fail even when only plain text was wanted.
* Two prompts now encode the same policy (the MCP macro `instructions` literal
  and `SERVER_INSTRUCTIONS`) because the `#[tool_handler]` macro rejects a const;
  they can drift.

---

## 14:41 — Follow-up: wrap long lines, split the layout module, document the demo

### Objective

Two follow-ups requested after the Phase 3 review:

1. implement the word-wrap item left open in `TASKS.md` — rendered answers were
   emitted as single long lines and left to the terminal, so a wrapped fenced
   code line lost its `  │ ` gutter alignment;
2. update `README.md` to show the demo media that had just been added
   (2 GIFs, 3 PNGs) and to state the machine it runs on — a local
   **RTX 4060 Ti (16 GB)** with **`gemma4:e4b`** via Ollama — including why a
   print-once CLI feels fast next to a full-fledged TUI.

### Solution

**Wrapping.** `ui/markdown.rs` was split into two responsibilities, because it
had grown to 555 lines:

| file | role |
| :- | :- |
| `ui/markdown.rs` (280) | *what* a line says: block detection + inline parsing → styled runs |
| `ui/wrap.rs` (301) | *where* it breaks: word wrap for prose, hard wrap for code |

Design choices:

* **Runs, not text.** Inline markdown is parsed into `Vec<Run>` (`text` +
  `Style`), so wrapping never has to re-parse or lose styling.
* **Prose word-wraps.** Continuation lines take a `next` prefix aligned under the
  text: bullets indent by two, `1. ` by the marker width, quotes by two, nested
  lists by `2 × depth`.
* **Glued tokens stay glued.** `` `code`, `` and `**bold**!` keep their comma or
  `!`, so the wrapper cannot invent or drop a space (an early version printed
  `bold text ,` and `italic textand a` — both caught with the preview harness).
* **Code hard-wraps.** Columns matter in code, so it breaks at cell boundaries and
  prefers the last space on the line (`break_at_space`); the continuation line
  gets four spaces so the code stays aligned under the first line's code.
* **Width `0` disables wrapping.** `ui::wrap_width` returns the terminal width on
  a tty and `0` when redirected, so `ask … > answer.md` keeps the model's own line
  structure — verified: a 160-character line stayed on one line when piped.
* Blank lines are collapsed to at most one, so a closing fence followed by a blank
  source line no longer leaves a double gap.

**README.** New `## Demo` section: both GIFs, a three-row screenshot table, the
local-hardware paragraph and a "Fast, because this is a CLI and not a full-fledged
TUI" subsection with measured timings.

**Media.** One screenshot was named `mcp_obisidan_chat.png` (transposed letters);
renamed to `mcp_obsidian_chat.png` before it reached the README.

### Actions Performed

1. `python3` pty harness (`/tmp/pty_run.py`, deleted afterwards) to run the binary
   with a real 70-column terminal, since wrapping is disabled when redirected —
   confirmed bullets, quotes, headings and backlinks wrap with aligned
   continuation lines.
2. `examples/preview_markdown.rs` (temporary harness, deleted afterwards) to render
   a crafted markdown sample at widths 72/60/40/30/0 and inspect edge cases:
   nested lists, quotes, fenced code, an unbreakable 60-character token, glued
   inline runs.
3. Split `ui/markdown.rs` → `ui/markdown.rs` + `ui/wrap.rs` (`mod wrap;` is
   private to `ui`).
4. `cargo build --release`, `cargo clippy --release --all-targets` (clean),
   `cargo test --release` (22 + 10 passed, 1 ignored).
5. Re-measured timings for the README on this machine: index a 490-note vault
   62 ms, vault query 59 ms (`nvidia-smi`: RTX 4060 Ti, 16380 MiB).
6. Updated `README.md`, `SUMMARY.md`, `ARCHITECTURE.md` (module map, line counts,
   "where to look if…"), `TASKS.md` (wrap item ticked).

### Verification Output

```
$ COLS=70 python3 /tmp/pty_run.py ./target/release/mcp_obsidian \
      --vault ~/rust/test_vault search term "list comprehensions" --limit 2 --max-chars 320
─────────────────── terms · list comprehensions ────────────────────
list comprehensions
• path: python/python_oneliners/list comprehensions.md
• backlinks: python/for loop.md, python/ml cookbook/ml cookbook 5
  handling categorical data.md, python/python_oneliners/list map
  slice.md, python/slim review.md

$ ./target/release/mcp_obsidian … search term … | awk '{print length}'
78 · 0 · 55 · 19 · 54 · 45 · 36 · 160      ← piped: no hard wraps injected

timings (RTX 4060 Ti, gemma4:e4b): vault index 62 ms · query 59 ms
cargo test --release → 22 passed + 10 passed, 1 ignored
cargo clippy --release --all-targets → no warnings
```

### References Used

1. `richrs` — `Segment::split_at` / `cell_length` (cell-accurate splitting and
   width measurement used by the wrapper):
   <https://docs.rs/richrs/0.2.1/richrs/segment/struct.Segment.html>
2. Unicode Annex #11 — *East Asian Width* (why width is measured in cells and not
   `chars().count()`): <https://www.unicode.org/reports/tr11/>
3. CommonMark Spec — *Line endings* and block structure (which source line starts
   a new block, which one is a continuation):
   <https://spec.commonmark.org/0.31.2/#blocks-and-inlines>

### Optional steps

* Add `ui/wrap.rs` cases to a test file using a recording `Console` — the wrapper
  is now the most layout-sensitive code in the crate and the preview harness that
  validated it was temporary.
* Support a `--width` flag (or honour `COLUMNS`) for reproducible screenshots and
  for piping into pagers that want pre-wrapped text.
* Render markdown tables (`richrs::Table`), which are still emitted as raw `|`
  lines and now wrap like prose.
* Compress the demo GIFs (49 MB + 38 MB) with `gifsicle -O3 --lossy=80` or record
  at a lower frame rate before committing them; GitHub accepts them, but a clone
  pays for every megabyte.

### Possible problems

* The wrapper drops repeated spaces inside prose and inside inline code spans
  (`a   =   b` becomes `a = b`), because words are re-joined with single spaces.
  Code *blocks* are unaffected — they hard-wrap verbatim — but an inline code span
  with significant spacing is not faithful.
* Wrapping depends on `Console::is_terminal()`; a terminal emulator that reports a
  wrong size (or a multiplexer without a proper `TIOCGWINSZ`) will wrap to that
  wrong width. `NO_COLOR` does not change the width, only the colour.
* Word wrap is greedy: it does not balance lines, so a paragraph can end with a
  long trailing word and a short last line.
* `break_at_space` searches back only within the current line, so a code line
  whose first token alone exceeds the width still splits mid-token.
* Two modules now share the `Segments` type; a future change to the `Run` shape
  touches `markdown.rs`, `wrap.rs` and every caller of `markdown::line`.

---

## 15:18 — Follow-up: make the demo GIFs repo-friendly (lossless re-encode)

### Objective

Decide whether the two demo GIFs (46.8 MB + 36.7 MB) should be hosted outside the
repository, and how `gifsicle -O3 --lossy=80` compares — with the constraint that
the frame rate must not be lowered.

### Solution

Measured instead of guessed. gifsicle is not installed on this machine and `apt`
needs a password, so gifsicle 1.95 was built from source into `/tmp/gifsicle-bin`
(no root). Both GIFs were copied and re-encoded with `-O1`, `-O3`,
`-O3 --lossy=80` and `-O3 --lossy=80 --resize 1280x720`, then verified with a
hand-written GIF block walker (frame count + per-frame delay sequence) and with
Pillow (composited pixel comparison).

Findings:

* The recordings are ~24 fps but almost static: `ask.gif` has **3** distinct
  visual states across 41.67 s and `chat.gif` **7** across 24.08 s. The recorder
  wrote a full 1920×1080 frame (47.9 KB) even when nothing changed, which is why
  the win is so large (0.14 KB/frame afterwards).
* `-O1` is **bit-identical**: same frame count, byte-identical delay sequence, and
  pixel Δ = 0 on every sampled frame. 46.8 MB → 143.8 KB and 36.7 MB → 406.4 KB.
* `-O3` additionally *merges identical consecutive frames* (1001 → 4, 579 → 9)
  with the total duration unchanged (41.67 s / 24.08 s): smaller files, same
  playback, lower stored frame count.
* `-O3 --lossy=80` only buys 10–25 % more than `-O1`/`-O3` and **does** change
  pixels (max channel Δ 51–66 on 20k–148k pixels), so it was rejected for flat
  Tokyo Night colours.

Decision: keep both GIFs **in the repo** (550 KB total, less than the three PNGs
combined) and re-encode them in place with `-O1`. No external hosting, no
git-lfs, relative README paths unchanged.

### Actions Performed

1. Built gifsicle 1.95 from source: `./configure --prefix=/tmp/gifsicle-bin`,
   `make -j4`, `make install` in `/tmp/gifsicle-1.95`.
2. Inspected both files: 1920×1080, 1001/579 full-size frames, 24 fps
   (delays 4–5 units), 47.9 KB/frame.
3. Ran the four variants on copies in `/tmp/giftest` and discovered that the
   frame counts reported by a naive read were wrong for `-O2`/`-O3` — they were
   *merging duplicates*, which a delay-sequence check proved (total time
   preserved), not dropping frames.
4. Compared pixels with Pillow after compositing frames (extracting single frames
   with gifsicle does not composite deltas, which produced a false "differs"
   result first time round).
5. Backed up the originals to `/tmp/gif_backup/`, re-encoded in place with
   `gifsicle -O1 <file> -o /tmp/<file>.out && mv /tmp/<file>.out <file>`.
6. Re-verified against the backups, removed the variant test directory, kept the
   compressed outputs and the backups.

### Verification Output

```
file                          frames       duration      size
demo_mcp_obsidian_ask.gif     1001 -> 1001  41.67 s   46.82 MB -> 143.8 KB   delays identical: True   pixel max Δ: [0,0,0,0]
demo_mcp_obsidian_chat.gif     579 ->  579  24.08 s   36.65 MB -> 406.4 KB   delays identical: True   pixel max Δ: [0,0,0,0]

variant comparison (ask.gif / chat.gif)
  original                      1001 / 579 frames   41.67 / 24.08 s   46.82 MB / 36.65 MB
  gifsicle -O1 (lossless)       1001 / 579 frames   41.67 / 24.08 s   143.8 KB / 406.4 KB
  gifsicle -O3 (merges dups)       4 /   9 frames   41.67 / 24.08 s   114 KB   / 358 KB
  -O3 --lossy=80                   4 /   9 frames   41.67 / 24.08 s   101 KB   / 332 KB   (pixels change)
  -O3 --lossy=80 --resize 720p     4 /   9 frames   41.67 / 24.08 s    60 KB   / 198 KB

runtime: ~15 s per file (ask) and ~9 s (chat) for -O1
```

### References Used

1. gifsicle manual — optimization levels (`-O1`/`-O2`/`-O3`) and `--lossy`:
   <https://www.lcdf.org/gifsicle/man.html>
2. GIF89a specification — Graphic Control Extension, delay and disposal
   semantics (what "frame rate" actually means in a GIF):
   <https://www.w3.org/Graphics/GIF/spec-gif89a.txt>
3. GitHub Docs — *About large files on GitHub* (repository and push size limits,
   when LFS is needed):
   <https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github>

### Optional steps

* Add a `media/` folder plus a `make media` target that runs `gifsicle -O1` over
  it, so a future recording cannot be committed unoptimized (87 MB is one
  `git add .` away from a ruined clone).
* If a future demo exceeds ~2 MB, add `--resize 1280x720` (halves the size; text
  stays legible) before considering external hosting.
* Record future demos at 1280×720 and trim idle time — the current files are
  mostly static, which is a content issue compression cannot fix.
* Install gifsicle from the distro package when convenient; the `/tmp` build
  disappears on reboot.

### Possible problems

* The originals currently live only in `/tmp/gif_backup/` (by request) — `/tmp` is
  cleared on reboot, so that safety net is temporary. The re-encoded files are
  bit-identical at every sampled frame, but the check was sampled, not exhaustive
  over all 1580 frames.
* gifsicle `--lossy` semantics depend on the version (≥ 1.92); a different build
  could produce different numbers than the table above.
* Anyone re-recording a demo needs gifsicle (or LFS/threshold knowledge); the
  repo has no tooling or documentation for that yet.
* `-O3`'s frame merging is only this dramatic because the recordings are static.
  A continuous screencast will not shrink as much, so the 550 KB result should not
  be used as an expectation for future demos.
