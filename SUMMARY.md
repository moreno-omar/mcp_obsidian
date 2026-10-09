# SUMMARY.md — mcp_obsidian (Phase 3 complete)

Last updated: 2026-10-09 19:10

How the CLI works, why it is built this way, and what a simpler version would need.
Goal: after reading this you should be able to rebuild a smaller working version
yourself.

**Phase 3 (reliable, readable answers) landed on 2026-10-09.** Grounded answers,
a deterministic pre-search and a Tokyo Night terminal UI. Start at
[§0](#0-phase-3--grounded-answers-and-tokyo-night-output-2026-10-09).
**Phase 2 (refactor) landed on 2026-10-01** — see [§0.1](#01-phase-2--the-refactor).
For *how to navigate* the code (reading order, module map, "where to look if…"),
see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## 0. Phase 3 — grounded answers and Tokyo Night output (2026-10-09)

Phase 3's exit criterion is an answer to *"how does one do list comprehensions
in python?"* built from a vault note that contains it. Two things blocked that.

### Problem 1 — `ask` often did not use the tools

Reproduced with `--trace` against a 490-note vault: the same question returned
`rounds: 1 | tool calls: 0` in 3 of 5 runs. In those runs the model answered from
its own knowledge — once word for word: *"my capabilities are limited to
searching and retrieving information from the provided Obsidian markdown vault,
and Python programming syntax is not content stored within the vault"* — even
though `python/python_oneliners/list comprehensions.md` exists.

Diagnosis was done by measuring the request in isolation (same prompt, same tool
definitions, straight to `/v1/chat/completions`):

| configuration | tool call in round 1 |
| :- | :- |
| old system prompt | 1/3 |
| + mandatory "search before answering" prompt | 4/6 |
| `tool_choice: "required"` | 1/3 — Ollama's OpenAI endpoint ignores it |
| tool schemas stripped of `$schema` / `format` / `default` | 2/4 |
| `temperature: 0` | 0/4 — it deterministically answers directly |

So the cause is **model behaviour, not the request**: `gemma4:e4b` decides
case-by-case whether a question is "vault-worthy", and no request field forces
that decision. Two changes fix it:

1. **An imperative protocol.** `SERVER_INSTRUCTIONS` — shared by the host system
   prompt *and* the MCP `instructions` field — plus every tool description now
   says *always search the vault before answering, never from your own
   knowledge*, and `search_by_term` tells the model to pass the subject words as
   separate terms (`["list", "comprehensions", "python"]`) instead of one phrase.
2. **A deterministic pre-search.** Before any model call, `Host::prefetch_notes`
   runs the prompt's subject words (stopword-filtered, ≤8 terms) through the same
   retrieval core and injects the top 3 hits into the system prompt. It takes
   milliseconds and needs no model, so the relevant notes are *always* in
   context, even for a model that never calls a tool. `--no-prefetch` disables it.

Result after the change: 5/5 runs grounded in the vault, 0 refusals. One of those
runs answered from the pre-searched notes *without* calling a tool at all — the
safety net doing exactly its job. Adding one sentence to the prompt ("call
`get_note` when an excerpt ends in `…[truncated]`") and widening `PREFETCH_CHARS`
to 800 then gave 3/3 runs that called a tool and cited the note path in a
340–460 word answer.

### Problem 2 — the output was raw text

New module `src/ui/` renders through `richrs` with a Tokyo Night palette:

```
src/ui/mod.rs        consoles, rules, panels, tables, per-command views
src/ui/theme.rs      the palette: RGB colours + semantic Style helpers
src/ui/markdown.rs   markdown → styled runs (headings, bullets, quotes,
                     fenced code, inline code / bold / italic / links)
src/ui/wrap.rs       runs → wrapped lines: word wrap for prose, hard wrap for
                     code that breaks at the last space of the line
```

Details that matter for a CLI like this one:

* `ui::stdout_console()` / `ui::stderr_console()` disable colour when the stream
  is not a terminal or `NO_COLOR` is set, so `| grep`, `> file` and the MCP
  **stdout** JSON-RPC stream stay byte-clean. `FORCE_COLOR`/`CLICOLOR_FORCE`
  force colour back on (used to verify the ANSI output while developing).
* A closed stdout (`cmd | head`) now exits 0 instead of reporting an I/O error.
* `serve` prints a startup panel to **stderr** only: vault, note count and the
  four tool names; `serve -vv` logs every MCP tool call.
* `ask` renders the model's markdown answer (headings, bullets, fenced code,
  inline code, emphasis, links) under the same theme; `--trace` shows the
  pre-search, each tool call and the round summary on stderr.
* Prose is **word-wrapped** to the terminal width with continuation lines aligned
  under the text (not under the bullet or the quote bar); code blocks are
  hard-wrapped at cell boundaries, breaking at the last space of the line, so
  columns survive. A width of `0` disables wrapping, and redirected output uses
  it, which keeps `ask "…" > answer.md` byte-faithful.
* `chat` gets a welcome panel, a teal `you>` prompt, a blue `llm>` label and the
  same answer rendering.
* `doctor`, `index` (now a table), `search` and `tools` were restyled too.

---

## 0.1 Phase 2 — the refactor

Phase 1 produced working code in five files, two of which had grown past the
point where you can hold them in your head. Phase 2 changed *structure only*:
not one byte of CLI, tool or prompt output changed (proof in §3).

### What moved where

```
src/lib.rs                 the crate's public surface: vault, ollama_client, mcp_server, host
src/main.rs                thin CLI (clap) — argument parsing plus printing

src/vault/                 was: vault.rs (1060 lines)
├── mod.rs                 module docs, defaults (DEFAULT_LIMIT …), re-exports
├── note.rs                Note, VaultStats, Hit, Direction            (data model)
├── index.rs               Vault: scan → indices → name resolution     (construction)
├── walk.rs                which files/dirs a scan includes            (fs policy)
├── parse.rs               markdown → Note: frontmatter, links, tags   (parsing)
├── query.rs               impl Vault: search_by_tag/term/linked_notes (retrieval)
├── render.rs              impl Vault: render_hits/note/index          (output)
└── text.rs                pure string helpers (normalise, score, truncate)

src/ollama_client/         was: ollama_client.rs (566 lines)
├── mod.rs                 module docs + re-exports
├── types.rs               constants, ApiFlavor, ChatMessage, ToolCall, ToolSpec
├── client.rs              OllamaClient: endpoints, version, models, chat
├── wire.rs                internal types → JSON request body (both dialects)
└── parse.rs               either dialect's reply → AssistantTurn
```

### Why it is shaped that way

**One `impl Vault` per concern, in its own file.** Rust lets you write several
`impl` blocks for the same type in different modules of the same crate, so
`query.rs` and `render.rs` each hold a small `impl Vault` while the struct and
its fields stay defined once in `index.rs`. Reading `query.rs` no longer means
scrolling past frontmatter parsing to find the ranking rules.

**Helpers are free functions in `text.rs`.** They have no `Vault` state to hold —
just `&str`/`&Note` in, `String`/`u32` out — so they do not need to be methods.
Keeping them in one file makes it obvious that tag normalisation, term scoring
and truncation are shared by parsing, querying and rendering.

**The library is the product, the binary is a shell.** `lib.rs` exposes the four
modules and `main.rs` only parses arguments and prints. That is what makes
integration tests possible at all: a binary-only crate cannot be linked from
`tests/`. Cost: the crate now compiles twice (lib + bin), which is free here.

**Behaviour was frozen before it was moved.** The tests were written against the
public API, then the refactor was checked two ways: `cargo test`, and a
side-by-side diff of 27 CLI invocations and a full MCP stdio session against a
`git worktree` build of the pre-refactor commit — byte-identical (§3).

### The tests

| file | tests | covers |
| :- | :- | :- |
| `tests/vault_queries.rs` | 22 | stats/index rendering, tag search (incl. nested + `match_all`), term search + ranking, graph walks in all three directions with depth, every note-address type, error messages, rendering and determinism |
| `tests/ollama_client.rs` | 10 + 1 ignored | client construction and URL normalisation, `model_available`, request bodies, both response dialects, HTTP error and unreachable-server paths; the ignored test does a real round trip (`cargo test -- --ignored live_client`) |

No new dependencies: the Ollama tests run against a ~70-line HTTP server built
on `std::net::TcpListener`, so both wire dialects are tested offline. `vault`
tests use `samples/vault`, which is the same fixture the README uses.

---

## 1. What exists now

A single Rust binary, `mcp_obsidian`, that does two jobs with one retrieval core:

1. **Local host loop** — `ask` / `chat`: prompt → Ollama → (model calls vault tools)
   → grounded answer.
2. **MCP server** — `serve`: the same four tools exposed to external MCP hosts over
   stdio.

```
src/lib.rs           the crate's public surface (vault, ollama_client, mcp_server, host, ui)
src/main.rs          CLI (clap): doctor, index, search, tools, ask, chat, serve
src/vault/           vault scan → in-memory index → 4 deterministic queries
src/mcp_server.rs    MCP tools (#[tool]) + shared tool specs/dispatch
src/ollama_client/   HTTP to Ollama, two wire dialects → one internal type
src/host.rs          the LLM ↔ vault loop, including the deterministic pre-search
src/ui/              presentation only: Tokyo Night theme + markdown renderer
```

Data flow, no model in the loop:

```
vault dir ──scan──> Vault { notes, tag_index, term_index, links, backlinks }
                                                            │
              search tag|term|links|note ────────────────────┘  (pure, in-memory)
```

Data flow, with a model:

```
prompt ─┬─> prefetch: subject words ─> Vault search (deterministic, ms)
        │                                     │
        └─> Ollama (tools[] + pre-searched notes in the system prompt)
                 │
                 ├─> tool_calls ─> ObsidianServer::dispatch ─> vault
                 │                        │
                 │                        └─> tool results (as text)
                 │
                 └─> answer ─> src/ui: markdown + Tokyo Night ─> stdout
```

---

## 2. Important steps and why

### Step 1 — Verify the SDK before writing code

`Cargo.toml` pinned `rmcp = "0.3"` (copied from the MCP docs). crates.io shows
`3.5.0` as the current stable, and the 0.3-era snippet no longer compiles:
`ServerInfo` was replaced by `ServerConfig` (an alias for `InitializeResult`), and
`#[tool_handler]` now generates `get_info` for you.

**Why it matters:** guessing the API wastes a build-fix cycle per call. Reading the
vendored crate source (`.cargo/registry/src/.../rmcp-3.5.0/src/`) gave the exact
signatures for `ToolRouter`, `ServerCapabilities::builder()`, `Parameters<T>` and
`transport::io::stdio()` up front. Final `Cargo.toml`: `rmcp 3.5`, `tokio 1.53`,
`reqwest 0.13`, `clap 4.6`, `walkdir 2.5`, `serde 1.0`, `anyhow 1.0`, `tracing 0.1`.

### Step 2 — Build the index once, query it purely

`Vault::load` walks the vault with `walkdir` (hidden dirs and `.obsidian`,
`node_modules`, `target` pruned), parses each `.md` file, then builds:

| index | purpose |
| :- | :- |
| `aliases` | path / file stem / frontmatter alias → note |
| `tag_index` | tag → notes (also serves nested tags) |
| `term_index` | lowercased word → notes (pre-filter so no full scan) |
| `outgoing` / `incoming` | resolved wikilink graph, both directions |

**Why it matters:** one pass produces every structure the four tools need, and every
query afterwards is pure — same vault + same query = same bytes. That is what makes
"deterministic and reproducible" (SPEC.md) testable without a model.

Hand-rolled frontmatter parsing (scalars, inline `[a, b]`, `- item` lists) was kept
instead of pulling in a YAML crate: the required shape is small and known, and it
avoids a dependency. Trade-off recorded under *Possible problems*.

### Step 3 — Split retrieval from the MCP surface

`src/vault.rs` holds the queries; `src/mcp_server.rs` holds request structs plus
`run_*` functions; the `#[tool]` methods and `ObsidianServer::dispatch` both call
those. Result: the MCP host and the local LLM loop execute *identical* logic.

**Why it matters:** `ToolRouter::call()` needs a `ToolCallContext`, which needs a
live `RequestContext<RoleServer>` — not constructible outside a running service.
Rather than spawn a second process and speak JSON-RPC to ourselves, the host calls
the shared functions directly. `serve` still exposes the same tools to real MCP
clients.

### Step 4 — One schema, two consumers

Tool parameter structs derive `schemars::JsonSchema`. `tool_specs()` runs
`schema_for!` over the *same* structs to build the Ollama `tools` array.

**Why it matters:** the MCP `inputSchema` and the model's `function.parameters`
cannot drift; adding a field updates both.

### Step 5 — Normalise Ollama's two dialects

`/v1/chat/completions` returns `arguments` as a JSON **string** and needs call ids;
`/api/chat` returns an **object** and omits ids. `ollama_client.rs` parses both into
`AssistantTurn { content, tool_calls }`, synthesising `call_N` ids when missing.

**Why it matters:** everything above that module is dialect-agnostic, so both APIs
are supported (and were both tested) with no conditional logic in the host.

### Step 6 — Keep stdout sacrosanct

`init_tracing` pins `tracing` to stderr for every subcommand. A stdio MCP server
that writes one stray byte to stdout corrupts the JSON-RPC stream.

### Step 7 — Verify each layer without the layer above it

| check | command |
| :- | :- |
| indexing | `index` |
| tag / nested tag | `search tag project`, `search tag project/alpha` |
| full text (multi-word) | `search term "Beta project"` |
| graph (both directions) | `search links index --depth 2` |
| by path, stem, alias, fuzzy | `search note projects/alpha`, `note beta`, `note home` |
| MCP protocol | `serve` + piped `initialize` / `tools/list` / `tools/call` |
| Ollama reachability | `doctor` |
| LLM round trip | `ask ... --no-tools` |
| LLM + tools | `ask ... --trace` |

---

## 3. Verified output

### Phase 1 — 2026-09-28

`doctor`:

```
ollama  version: 0.33.1 (reachable at http://localhost:11434 /api/version)
        models: 12 found
        gemma4:e4b is available
vault   4 notes, 11 tags, 12 headings, 10 links (0 unresolved)
status: ready
```

`ask --no-tools`, 1 round, 7.8 s → `pong`.

`ask "why does this project avoid embeddings?"` (OpenAI flavour, 3 rounds, 2 tool calls):

```
tool: search_by_term {"terms":["embeddings","project"]}
tool: get_note {"note":"notes/retrieval-design.md"}
→ answer cites notes/retrieval-design.md
```

`ask --api native "which note is about the follow-up project after Alpha?"`
(2 rounds, 1 tool call, 7.9 s) → `projects/beta-note.md` + two of its ideas.

MCP handshake over stdio: `initialize` negotiates protocol `2025-11-25`, advertises
`tools`, `tools/list` returns 4 tools with full JSON Schema, `tools/call` returns
`content[].text` and `isError: false`.

### Phase 2 — 2026-10-01 (refactor)

`cargo test` — 32 tests, 0 failures, 1 ignored:

```
running 22 tests   (tests/vault_queries.rs)   test result: ok. 22 passed; 0 failed
running 11 tests   (tests/ollama_client.rs)   test result: ok. 10 passed; 0 failed; 1 ignored
```

`cargo clippy --all-targets` — no warnings. `cargo build --release` — clean.

Behaviour-preservation check: the pre-refactor commit (`d143c00`) was built in a
separate `git worktree`, then both binaries were run over the same vault and the
outputs diffed. All 27 CLI invocations (`index`, every `search` kind and
direction, every note-address form, and the error paths) are **byte-identical**,
as is a full MCP stdio session (initialize → tools/list → 3× tools/call).

Largest source file before → after: `vault.rs` 1060 → 318 lines (`index.rs`);
`ollama_client.rs` 566 → 235 (`client.rs`). Every module is now well under one
screen of code.

### Phase 3 — 2026-10-09 (grounding + UI)

`cargo clippy --release --all-targets` — no warnings. `cargo test --release` —
`tests/vault_queries.rs` 22 passed, `tests/ollama_client.rs` 10 passed + 1
ignored (the live-model test). No vault or tool payload changed: `render_hits`,
`render_note` and `render_index` are byte-identical (their tests still pass), so
MCP hosts see exactly the same text as before — only the CLI's own presentation
is new.

The exit criterion, `~/rust/test_vault` (490 notes), 5 consecutive runs:

```
$ ./target/release/mcp_obsidian --vault ~/rust/test_vault \
    ask "how does one do list comprehensions in python?" --trace
  · prefetch search_by_term [list, comprehensions, python]
  · get_note {"note":"python/python_oneliners/list comprehensions.md"}
rounds: 2 | tool calls: 1            # 5/5 runs grounded, 0 refusals
```

The same question before the fix: `rounds: 1 | tool calls: 0` and a refusal
("my capabilities are limited to searching … the vault") in 3 of 5 runs.

MCP server, stdout kept pure (`initialize` → `tools/call`), banner on stderr:

```
╭─ mcp_obsidian · serve ───────────────────────────────────────────────────────╮
│ transport stdio (stdout = JSON-RPC)                                          │
│ vault     /…/samples/vault                                                   │
│ indexed   4 notes, 11 tags, 0 unresolved links                               │
│ tools     search_by_tag, search_by_term, get_linked_notes, get_note          │
╰──────────────────────────────────────────────────────────────────────────────╯
```

---

## 4. Using it

```bash
cargo build --release                     # binary at target/release/mcp_obsidian

# no model needed
./target/release/mcp_obsidian --vault ~/vault index
./target/release/mcp_obsidian --vault ~/vault search tag project --match-all
./target/release/mcp_obsidian --vault ~/vault search term "token budget"
./target/release/mcp_obsidian --vault ~/vault search links index --depth 2

# with Ollama
./target/release/mcp_obsidian --vault ~/vault doctor
./target/release/mcp_obsidian --vault ~/vault ask "what do my notes say about token budgets?" --trace
./target/release/mcp_obsidian --vault ~/vault ask "…" --no-prefetch   # tools only, no pre-search
./target/release/mcp_obsidian --vault ~/vault chat
```

As an MCP server (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "/home/redram/rust/mcp_obsidian/target/release/mcp_obsidian",
      "args": ["--vault", "/home/redram/vault", "serve"]
    }
  }
}
```

Configuration can also come from the environment: `OBSIDIAN_VAULT`, `OLLAMA_URL`
(default `http://localhost:11434/v1`), `OLLAMA_MODEL` (default `gemma4:e4b`),
`RUST_LOG`. `--api native` switches from `/v1/chat/completions` to `/api/chat`.

Output is coloured only when the stream is a terminal: set `NO_COLOR=1` to turn
it off explicitly, `FORCE_COLOR=1` (or `CLICOLOR_FORCE`) to force it on when
piping. `--trace` writes the pre-search, the tool calls and the round summary to
stderr, so `ask … > answer.md` still produces a clean markdown file.

Tests need no model (the one test that does is ignored):

```bash
cargo test                            # 32 tests: vault queries + ollama client
cargo test -- --ignored live_client   # needs `ollama serve` and the model pulled
```

---

## 5. Suggestions (to improve)

1. **Return `CallToolResult::error` for real failures.** Today a failed tool returns
   the text `ERROR: …`, which the model recovers from but an MCP client renders as
   success. Map "no such note / bad argument" to an error result and keep plain text
   for "nothing matched".
2. **Persist the index.** Re-scanning the vault on every launch is the only cost
   that grows with vault size. Serialise the index (with file mtimes) and reload
   when nothing changed.
3. **Add a `resources` capability.** Exposing notes as MCP resources (not just tool
   results) lets a host show and pin individual notes without a tool round trip.
4. **Heading-scoped retrieval.** `beta-note.md` already asks for it: let the model
   request one `## section` instead of a whole note, so the token budget is spent
   on the relevant part.
5. **Drop `walkdir`** if you want the dependency set minimal — a sorted recursive
   `read_dir` is ~15 lines; `walkdir` was kept for symlink-cycle safety and ordered
   traversal.
6. **Link-rot report.** `doctor` already counts unresolved wikilinks; add a
   `links --check` subcommand listing them per note.
7. **Escape hatch for the host.** The loop is capped by `--max-rounds`; a hard cap
   on total characters sent back to the model would bound cost more predictably.
8. **Split `main.rs` next.** It is now the largest file (513 lines) and the last
   one that mixes concerns: clap types, `run_search` dispatch, `build_host`,
   `run_serve`, `run_doctor` and `init_tracing`. `cli.rs` + `commands.rs` would
   bring it to the same size as the rest. (Tracked in TASKS.md.)
9. **Golden-file tests for rendering.** The exit-criteria for Phase 2 is
   "same functionality"; that is currently proven by assertions plus a manual
   A/B diff. Committing the old binary's output as `tests/golden/*.txt` and
   diffing against it would make the guarantee permanent and automatic.
10. **Property test for the parsers.** `parse.rs` and `text.rs` are the two
   modules where a subtle input (a stray `---`, a tag inside a URL, a
   multi-byte character at a truncation boundary) could silently change output.
   A `proptest` round-trip over `split_frontmatter` / `extract_inline_tags`
   would cover far more inputs than the hand-written cases.
11. **Render tables and LaTeX-ish blocks.** The model already emits markdown
   tables and `$$…$$` formulas; today they fall through as plain text. A
   markdown table → `richrs::Table` conversion would be the single most visible
   improvement left.
12. **Use model capabilities to tune the request — and measure once more.**
   `gemma4:e4b` is a *thinking* model, and `"think": false` looked faster
   (2.5–5.6 s vs ~10 s with tools) and more tool-call-prone (3/4 vs ~40%) in the
   diagnosis runs. A later, tool-free measurement however showed the default
   request at 6.7 s and `think: false` at 5.9 s, i.e. the OpenAI dialect is
   already *not* thinking unless `"think": true` is sent (12.8 s, `reasoning`
   populated). So the latency lever here is smaller than the first number
   suggested; `/api/show` reports `capabilities: […"thinking"]`, which is the
   clean way to decide, and the numbers above should be re-measured before
   wiring a flag.
13. **RAG-free semantic help.** When the pre-search finds nothing, a synonym the
   vault never uses still fails. A per-note keyword/alias list (frontmatter
   `aliases:` already exists in the parser) could soften that without
   embeddings.

## 6. Possible problems

1. **`AGENTS.md` still carries Python-flavoured rules.** Its stack section says
   Rust/Cargo, but the coding-style line asks for "production-grade **Python**
   code" and there is a "Python Execution Rules" section. Harmless while it is
   only guidance, but an automated agent following it literally could introduce
   Python tooling into this Cargo repo. Either drop those lines here or move them
   to the projects they apply to.
2. **Frontmatter parsing is deliberately shallow.** Nested YAML maps, anchors and
   multi-line block scalars are ignored, not modelled. A vault relying on those will
   silently lose metadata. Upgrade path: `serde_yaml`/`gray_matter` behind the same
   `split_frontmatter` interface.
3. **In-memory index scales with the vault.** Fine for thousands of notes; a
   multi-gigabyte vault pays the full parse cost on every launch.
4. **Fuzzy note resolution can be ambiguous.** A short name such as `beta` that
   substring-matches several notes produces an "ambiguous, use a full path" error
   listing the candidates. Correct behaviour, but the model may not retry with a
   full path on its own.
5. **Term scoring is lexical, not semantic.** A question using a synonym the vault
   never uses returns nothing. That is the intended design (no embeddings), but it
   makes prompt wording matter.
6. **Model quality gates the agent loop.** `gemma4:e4b` called the right tools in
   these tests, but nothing forces a model to read a tool result before answering;
   the system prompt asks it to cite paths and say when nothing matched.
7. **No auth, by design.** Read-only and local-only; `serve` will index whatever
   `--vault` points at. Do not point it at a directory with secrets unless you accept
   that an MCP client can read it.
8. **`rmcp` is pre-1.0.** 0.3 → 3.5 rewrote the server traits; the next minor bump
   may require another migration of `mcp_server.rs` (contained, ~250 lines).
9. **One `impl` block per file is a convention, not an invariant.** Rust is happy
   with `impl Vault` in `query.rs`, `render.rs` and `index.rs`, but nothing stops
   a later edit from putting query logic back into `index.rs`. If that matters,
   make `Vault`'s fields private to `index.rs` (they already are) and route every
   accessor through `index.rs` — the current split relies on that discipline.
10. **`term match:` can name the wrong term.** `search_by_term` labels a
    single-term match with `queries[0]` even when a later term is the one that
    matched, so a multi-word OR search can report a term the note does not
    contain. This is pre-existing (unchanged by the refactor) and is now visible
    in one place: `term_reason()` in `query.rs`. Fixing it changes tool output,
    so it was left alone deliberately.
11. **The CLI tests are a snapshot, not a gate.** The 27-invocation A/B diff was
    run by hand against a `git worktree` of `d143c00`. Nothing re-runs it; the
    `cargo test` suite is the only automatic protection. See suggestion 9.
12. **The pre-search is lexical and can mislead.** `prefetch_notes` picks the top
    3 notes for up to 8 stopword-filtered words. A question whose words are
    common in the vault (e.g. "what should I work on?") injects a semi-random
    slice, and a small model may anchor on it. `--no-prefetch` is the escape
    hatch; raise `PREFETCH_*` constants deliberately, not casually.
13. **Grounding is not verification.** The prompt now forbids answering from
    memory, but nothing checks that the answer cites a note the tools actually
    returned. A cheap guard would be to drop or flag answers whose note paths do
    not appear in the tool results.
14. **Colour detection trusts the environment.** `richrs` defaults to true colour
    unless `TERM` says otherwise, and `ui` disables colour only for a
    non-terminal or `NO_COLOR`. Terminals that are 8-colour (or Windows legacy)
    will still get 24-bit sequences unless `TERM` is accurate.
15. **`ask` is one-shot and stateless.** Each prompt rebuilds the message list,
    so follow-up questions in `ask` cannot refer to the previous answer (only
    `chat` has a loop, and it too rebuilds per turn). History would need to live
    in `Host`, plus a token budget to keep it bounded.
