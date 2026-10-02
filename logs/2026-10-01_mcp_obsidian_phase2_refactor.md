# 2026-10-01 — mcp_obsidian Phase 2 (refactor)

## 19:55 — Phase 2: split the monoliths, freeze behaviour with tests

### Objective

Implement every unchecked item in `TASKS.md` (Phase 2 "Refactor" of
`ROADMAP.md`), whose exit criterion is *"code separated into smaller and more
concise files that can be viewed mostly on 1 screen"*:

1. tests for the vault-only commands — index, tag, term, links, note;
2. tests that exercise the different ways a note can be addressed;
3. split `impl Vault` across modules grouped by concern;
4. move the helper functions into their own file;
5. tests for `OllamaClient` — create the client, connect to it, receive a response;
6. split `impl OllamaClient` across modules grouped by concern.

### Solution

Structure-only change. No CLI output, tool output, prompt or scoring rule changed;
that was enforced by diffing against a build of the pre-refactor commit.

New layout (old → new line counts):

| before | after |
| :- | :- |
| `src/vault.rs` (1060) | `src/vault/mod.rs` (39), `note.rs` (82), `index.rs` (318), `walk.rs` (69), `parse.rs` (282), `query.rs` (171), `render.rs` (112), `text.rs` (163) |
| `src/ollama_client.rs` (566) | `src/ollama_client/mod.rs` (32), `types.rs` (166), `client.rs` (235), `wire.rs` (104), `parse.rs` (111) |
| — | `src/lib.rs` (18) + thin `src/main.rs` |

Why each decision, in short:

* **Several `impl Vault` blocks, one per concern.** Rust allows inherent `impl`
  blocks for a local type in any module of the same crate, so `query.rs`
  (retrieval), `render.rs` (output) and `index.rs` (construction) each own a small
  block while the struct and its fields stay defined once. `Vault`'s fields remain
  private to `index.rs`; cross-module reads go through `notes()`, `note_at()`,
  `root()`.
* **Helpers are free functions in `vault/text.rs`.** They hold no vault state,
  so they do not need to be methods, and one file makes it obvious that tag
  normalisation, term scoring and truncation are shared by parsing, querying and
  rendering. `vault/walk.rs` holds the other kind of helper: the scan policy
  (which directories to prune, what counts as markdown).
* **`lib.rs` + thin `main.rs`.** Integration tests cannot link a binary-only
  crate. Moving behaviour into a library target is what made `tests/` possible;
  `main.rs` now only parses arguments and prints.
* **`ollama/parse.rs` vs `ollama/wire.rs`.** The two dialects differ in three
  places (shape of `tool_calls`, arguments as object vs JSON string, whether the
  id is echoed). Both directions of that translation now live in exactly one file
  each, which is what makes the "one schema, two consumers" claim checkable.

Tests added (32 total, no new dependencies):

* `tests/vault_queries.rs` (22) — statistics and index rendering, tag search
  including nested tags and `match_all`, term scoring/ranking, graph walks in all
  three directions with depth and hop reasons, **every address type** (vault
  path, `.md`, leading slash, file stem, title, frontmatter alias, Windows
  separators, `#heading` suffix, `|alias` suffix, fuzzy substring, plus unknown
  and ambiguous errors), rendering and determinism. Fixture: `samples/vault`.
* `tests/ollama_client.rs` (10 + 1 `#[ignore]`) — construction and URL
  normalisation, `model_available`, request bodies, both response dialects
  (including synthesised call ids), HTTP error and unreachable-server paths. They
  run against a ~70-line HTTP server built on `std::net::TcpListener`, so no live
  Ollama is needed. `cargo test -- --ignored live_client` does the real round trip.

### Actions Performed

1. Read `AGENTS.md`, `ROADMAP.md`, `TASKS.md`, `SUMMARY.md`, `SPEC.md` and all of
   `src/`; recorded the plan as todos in the session database.
2. `git worktree add /tmp/mcp_obsidian_baseline HEAD` and built the pre-refactor
   binary there, so the refactor could be diffed against real output rather than
   against my own expectations.
3. Created `src/vault/{mod,note,walk,parse,index,query,render,text}.rs` and
   `src/ollama_client/{mod,types,client,wire,parse}.rs`; deleted `src/vault.rs`
   and `src/ollama_client.rs`.
4. Added `src/lib.rs` exposing the four modules; rewrote `src/main.rs` to use
   `mcp_obsidian::…` instead of inline `mod` declarations.
5. Fixed the compile errors the split produced (7): private field access from
   sibling modules (`notes`/`root`) replaced with accessors, and one iterator
   mismatch in `text.rs`.
6. Wrote the two test files; corrected three test expectations that were wrong
   (not the code): `embeddings` ranks the heading match first, `["token",
   "embeddings"]` *does* match a note, and `beta-note` is already only one hop
   from everything.
7. Ran `cargo build`, `cargo test`, `cargo clippy --all-targets`,
   `cargo build --release`.
8. Ran the 27-invocation CLI diff and a full MCP stdio session against the
   baseline binary.
9. Updated `TASKS.md` (all six items checked, follow-ups listed), `ROADMAP.md`
   (Phase 2 status) and `SUMMARY.md` (§0 phase-2 explanation, new file map,
   §3 verification, new suggestions and possible problems).

### Verification Output

Tests — 32 passed, 1 ignored, 0 failed:

```
running 22 tests   (tests/vault_queries.rs)  test result: ok. 22 passed; 0 failed
running 11 tests   (tests/ollama_client.rs)  test result: ok. 10 passed; 0 failed; 1 ignored
```

Lint and release build:

```
$ cargo clippy --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s   # no warnings
$ cargo build --release
    Finished `release` profile [optimized] target(s) in 5.16s
```

Behaviour preservation — old binary vs new binary, same vault, byte-diffed:

```
ok  [index]            ok  [term-embeddings]   ok  [note-home]
ok  [index-limit]      ok  [term-matchall]     ok  [note-path]
ok  [index-limit0]     ok  [term-maxchars]     ok  [note-winext]
ok  [tag-project]      ok  [links-index]       ok  [note-heading]
ok  [tag-matchall]     ok  [links-alpha-out]   ok  [note-fuzzy]
ok  [tag-nested]       ok  [links-alpha-back]  ok  [note-ambiguous]
ok  [tag-none]         ok  [links-alpha-both]  ok  [note-missing]
ok  [tag-hash]         ok  [links-unknown]     ok  [note-maxchars]
ok  [term-multi]       ok  [links-baddir]      ok  [tools]
                       27 scenarios, 0 differences (stdout and exit codes)
```

MCP stdio session (initialize → tools/list → 3× tools/call, one of them an
error case) diffed between the two binaries: `IDENTICAL` for stdout and stderr.

File sizes after (`wc -l`): largest source file 318 (`vault/index.rs`), then
`mcp_server.rs` 308, `vault/parse.rs` 282, `ollama/client.rs` 235 — down from
`vault.rs` 1060 and `ollama_client.rs` 566.

Live checks against the Ollama instance running on this machine
(`ollama` 0.33.1, `gemma4:e4b` present):

```
$ cargo test --test ollama_client -- --ignored --nocapture
test live_client_connects_and_answers ... ok        # version + models + one real chat round
test result: ok. 1 passed; 0 failed; 0 ignored; 10 filtered out; finished in 52.46s

$ mcp_obsidian --vault samples/vault doctor
ollama  version     : 0.33.1 (reachable at http://localhost:11434 /api/version)
        models      : 12 found
        model check : gemma4:e4b is available
vault   indexes     : 4 notes, 11 tags, 12 headings, 10 links (0 unresolved)
status: ready

$ mcp_obsidian --vault samples/vault ask "what do my notes say about token budgets?" --trace
tool: search_by_term {"terms":["token budgets"]}
rounds: 2 | tool calls: 1
I couldn't find any notes that mention "token budgets."
```

The last one is the documented lexical limit (SUMMARY.md §6.5), not a regression:
the vault says "Token budget" (singular) and `search term` matches whole words, so
the plural query legitimately finds nothing — the same result the pre-refactor
code gives (`score_terms` returns 0 as soon as one word is absent). The model
tool-call path itself worked: it dispatched `search_by_term` and fed the result
back.

### References Used

1. Rust Book — *Separating Modules into Different Files*:
   <https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html>
2. Rust Book — *Test Organization* (unit vs `tests/` integration tests):
   <https://doc.rust-lang.org/book/ch11-03-test-organization.html>
3. Cargo Reference — *Cargo Targets* (library and binary targets in one package):
   <https://doc.rust-lang.org/cargo/reference/cargo-targets.html>

### Optional steps

* Split `src/main.rs` (414 lines, now the largest file) into `cli.rs` (clap
  types) and `commands.rs` (subcommand bodies). Tracked in `TASKS.md`.
* Turn the manual A/B diff into committed golden files (`tests/golden/*.txt`)
  and assert against them, so "same functionality" is checked by `cargo test`.
* Wire `cargo clippy -- -D warnings` and `cargo test` into CI (or a `justfile`),
  now that both are clean.
* `sqlite` was not introduced: the vault index is in memory by design (SPEC.md
  non-goal), so nothing here needs a database.

### Possible problems

* **The CLI/MCP equivalence proof is a one-off.** The 27-invocation diff and the
  stdio session were run by hand against a temporary worktree
  (`/tmp/mcp_obsidian_baseline`, removed afterwards). Nothing re-runs them, so
  suggestion "golden files" above is the real fix.
* **`term match:` can name the wrong term.** For a multi-word OR search, a note
  matching only the second term is still labelled with the first. Pre-existing
  behaviour, deliberately preserved (it is a documented output format), and now
  isolated in `text`/`query` helpers — see SUMMARY.md §6.10.
* **The `impl` split is a convention, not an invariant.** Nothing prevents query
  code from being added back into `index.rs`; keeping it clean relies on review.
* **No test covers `serve`.** The MCP stdio path was only checked manually; a
  regression there (e.g. a stray `println!`) would not be caught by `cargo test`.
* **`cargo test` compiles the crate three times** (lib, bin, each test binary).
  A few extra seconds today; worth watching if the crate grows.

## 20:00 — ARCHITECTURE.md: a reading guide for the refactored code

### Objective

The Phase 2 split turned two files into thirteen modules. The structure is now
good but no longer self-evident from a directory listing: which file to open
first, why `impl Vault` appears three times, and where a change belongs were all
tribal knowledge living in `SUMMARY.md` (which explains *why*, not *how to
navigate*). Target: an `ARCHITECTURE.md` that answers "how do I read this code
and where do I edit it", following the outline in `ARCHITECTURE_example.md`.

### Solution

Created `ARCHITECTURE.md` (repo root, ~16 KB) with the example's exact sections —
purpose, one-paragraph summary, entry point, reading order, module map, key
types, data/control flow, "where to look if…", glossary — filled in from the real
code rather than from memory:

* **Entry point** — `src/lib.rs` first (18 lines, the whole architecture), then
  `main.rs:162` `fn main`.
* **Reading order** — a 10-step table, each step depending only on the previous
  one, with a note on where to stop if you only care about the vault (steps 1–5)
  versus the outside world (6–8).
* **Module map** — a tree annotated with line counts and one-line
  responsibilities, grouped by layer (retrieval / tool surface / transport /
  orchestration).
* **Key types** — the 7 types that carry the design, plus an ASCII diagram
  showing that `run_*` in `mcp_server.rs` is the single choke point feeding the
  CLI, the `#[tool]` methods and `dispatch`.
* **Data/control flow** — the three paths through the code, including the full
  `ask` round trip, with the reasoning for the layering (`run_*` owns defaults
  and error text; `Vault` owns retrieval).
* **"Where to look if…"** — a 17-row task→file table. For example, "add a new MCP
  tool" lists all five edits (struct, `run_*`, `#[tool]`, `tool_specs`,
  `dispatch`), which is the least obvious thing in the codebase.
* **Glossary** — 14 domain terms (vault, wikilink/backlink, alias, term, hit,
  dialect, host loop, unresolved link…) as they are used *in this code*.

Every reference is a clickable relative link with a `#Lnn` anchor.

### Actions Performed

1. Read `ARCHITECTURE_example.md` and mapped its outline onto this project.
2. Re-derived every symbol's line number with `grep -n` over `src/` (no numbers
   from memory or from the earlier session notes).
3. Wrote `ARCHITECTURE.md`; rewrote the key-types ASCII diagram once, because the
   first version implied the CLI called the vault directly instead of through the
   shared `run_*` functions.
4. Verified with a Python script: all 37 `#Lnn` references resolve to the
   intended symbol (printed each referenced line and eyeballed it), all 23 linked
   paths exist, and all 15 declared line counts match `wc -l`.
5. Appended this log entry and added pointers to the new file from `SUMMARY.md`
   and `README.md`.

### Verification Output

Reference check (37 unique line anchors, sample):

```
src/main.rs: 162            | async fn main() -> Result<()> {
src/vault/index.rs:  45     | pub fn load(root: impl AsRef<Path>) -> Result<Self> {
src/mcp_server.rs: 177      | pub fn tool_specs() -> Vec<ToolSpec> {
src/host.rs:  63            | pub async fn ask(&self, prompt: &str) -> Result<AskOutcome> {
src/ollama_client/parse.rs: 26 | pub(super) fn parse_openai_response(body: &Value) -> … {
```

Link and size check — 23/23 paths exist, 15/15 declared line counts match:

```
OK   src/main.rs                  declared  414  actual  414
OK   src/vault/index.rs           declared  318  actual  318
OK   src/vault/parse.rs           declared  282  actual  282
OK   src/ollama_client/client.rs  declared  235  actual  235
OK   ... (all 15 checked, 0 differences)
```

### References Used

1. matklad — *ARCHITECTURE.md* (the pattern this file follows):
   <https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html>
2. Rust Book — *Defining Modules to Control Scope and Privacy*:
   <https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html>
3. Diátaxis — *Explanation vs Reference* (why this file is separate from
   SUMMARY.md and the rustdoc comments):
   <https://diataxis.fr/explanation/>

### Optional steps

* Add the same "where to look if…" table to `TASKS.md` as a preamble, so a new
  task starts with the right file already open.
* Make drift impossible: a tiny test that parses `ARCHITECTURE.md` and asserts
  every `#Lnn` anchor still names the symbol recorded beside it (the check run
  above is currently a throwaway script).
* Reference `ARCHITECTURE.md` from `lib.rs`'s crate docs (`//!` at the top) so
  `cargo doc --open` points a reader at the guide.

### Possible problems

* **Line anchors rot.** `ARCHITECTURE.md` pins 37 line numbers; any edit above a
  cited symbol silently invalidates one. Symbol *names* are given next to every
  anchor, so the doc stays usable by search, but the links will need re-checking
  after refactors (the check script should be committed — see Optional steps).
* **The "reading order" is an opinion, not a constraint.** It follows the data
  (`Note` → `Vault` → queries → rendering), which is right for a newcomer but not
  for someone who only wants the MCP layer; the stopping-point note mitigates
  this but a second, audience-based order may be worth adding.
* **Two documents now overlap.** `SUMMARY.md` (§0, §1) and `ARCHITECTURE.md` both
  describe the module layout. They are separated by intent (why vs how to
  navigate), but a future edit must remember to update both.
