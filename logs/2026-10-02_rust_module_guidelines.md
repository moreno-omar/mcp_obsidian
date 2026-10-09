# 2026-10-02 — mcp_obsidian — Rust module guidelines

## 15:26 — Documenting the module system behind `src/vault/mod.rs`

### Objective

Answer a recurring comprehension question — *does `pub use index::Vault;`
"use the implementation" in `index.rs`?* — and capture the answer as a reusable
document at `rust_module_guidelines.md`, covering:

1. how the Rust module system maps files to modules (`mod.rs` vs `foo.rs`);
2. what a `pub use` re-export actually does, and what it does **not** do;
3. the payoff, demonstrated at `src/main.rs:36`;
4. that multiple `impl Vault` blocks in different files merge into one type;
5. the facade pattern as applied to the `vault` module;
6. the `pub(crate)` pattern for internal helpers;
7. rules of thumb, best practices, worked examples and common mistakes.

### Solution

Wrote `rust_module_guidelines.md` (10 sections, ~425 lines) grounded entirely in
this repository's real files rather than generic prose. Every claim is tied to a
file and line: `src/vault/mod.rs:23-31` for the facade, `src/vault/index.rs:25,43`
for the definition, `src/vault/query.rs:14` and `src/vault/render.rs:13` for the
split `impl` blocks, `src/lib.rs:15-18` for the crate tree, `src/main.rs:36` for
the consumer benefit.

Key clarifications encoded in the doc:

* `pub use index::Vault;` is a **re-export** (`use` + `pub`). The type is defined
  once in `index.rs`; the statement only decides which *path* may name it
  (`crate::vault::Vault`, not the private `crate::vault::index::Vault`).
* Inherent `impl` blocks for a crate-local type may live in **any** module of the
  same crate and are merged by the compiler into one type. This is why
  `query.rs` and `render.rs` can add `&self` methods that read `Vault`'s private
  fields (field privacy is module-of-definition based, not file-global).
* `mod.rs` is an ordinary module body: it may define `pub struct`, `impl`, `fn`
  and `const` items. This project *chooses* to keep it declarative (40 lines),
  which is a convention, not a language rule.
* `pub(crate)` is the internal-seam visibility used for `parse_note`,
  `note_aliases`, `markdown_files` and `read_note`.

### Actions Performed

1. Read `src/vault/mod.rs`, `index.rs`, `parse.rs`, `note.rs`, `walk.rs`,
   `query.rs`, `render.rs`, `text.rs`, plus `src/lib.rs` and `src/main.rs` to
   collect the real paths, names, signatures and line numbers.
2. Built a throwaway crate at `/tmp/modtest` to empirically confirm the two
   claims the notes hinged on: a `pub struct` defined in `mod.rs`, and an
   `impl super::Foo` block living in a child module. Ran it with `cargo run -q`.
3. Removed `/tmp/modtest` (no scratch artifacts left behind).
4. Wrote `rust_module_guidelines.md` via a quoted heredoc (the file already
   existed but was empty, so `create` was not usable).
5. Re-verified every reference against the source and corrected six inaccuracies
   before finishing: `src/lib.rs` tree lines (15/16/17/18), `walk.rs` line
   numbers (29 and 47, not 28/48), the private-field example (`term_index`, not a
   non-existent `locations`), `search_by_tag`'s real signature
   (`&self, &[String], match_all: bool, limit: usize) -> Vec<Hit>`),
   `render_note` returning `String` (no `?`), and the layer-diagram wording for
   `walk.rs` (scan policy, not pure).
6. Ran a link check over the document's relative Markdown targets.

### Verification Output

Empirical proof that `mod.rs` may define types and children may extend them —
`cargo run -q` in the scratch crate:

```
mod.rs-defined Foo: 1 / ext method: 3
child module sees super::Foo with n=1
```

Link check over `rust_module_guidelines.md` (all targets resolve):

```
OK   logs/2026-10-01_mcp_obsidian_phase2_refactor.md
OK   src/lib.rs
OK   src/main.rs
OK   src/vault/index.rs
OK   src/vault/mod.rs
OK   src/vault/note.rs
OK   src/vault/parse.rs
OK   src/vault/query.rs
OK   src/vault/render.rs
OK   src/vault/text.rs
OK   src/vault/walk.rs
```

Source references used for the doc were confirmed with `grep -n`:

```
src/lib.rs          15:pub mod host;   16:pub mod mcp_server;
                    17:pub mod ollama_client;   18:pub mod vault;
src/vault/walk.rs   29:pub(crate) fn markdown_files(...)
src/vault/walk.rs   47:pub(crate) fn read_note(...)
src/vault/query.rs  17:pub fn search_by_tag(&self, tags: &[String], match_all: bool, limit: usize) -> Vec<Hit>
src/vault/render.rs 38:pub fn render_note(&self, note: &Note, body_chars: usize) -> String
```

### References Used

1. The Rust Programming Language — *Defining Modules to Control Scope and Privacy*: https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html
2. The Rust Reference — *Visibility and privacy*: https://doc.rust-lang.org/reference/visibility-and-privacy.html
3. Rust API Guidelines: https://rust-lang.github.io/api-guidelines/

### Optional steps

* Add `#![deny(rustdoc::broken_intra_doc_links)]` to `src/lib.rs` so the module
  map tables in `mod.rs`/`lib.rs` fail the build if a doc link rots.
* Add `cargo doc --no-deps` to whatever CI runs, as a cheap facade check.
* Consider snapshotting the public surface with `cargo public-api` so an
  accidental `pub` on an internal helper shows up as a review-time diff.
* Decide explicitly between `mod.rs` and the Rust 2018 `vault.rs` + `vault/`
  spelling; the repo mixes neither today but new modules could drift.

### Possible problems

* **Line numbers drift.** The document cites concrete lines (`index.rs:25`,
  `main.rs:36`, `query.rs:14`). Any refactor above those lines silently makes the
  doc wrong. Mitigation: prefer symbol names in prose, re-check on refactors, or
  lean on the doc-link checker above.
* **Convention vs rule confusion.** The doc states `mod.rs` may hold items, but
  the repo currently keeps it declarative; a future contributor could read the
  guidelines as a mandate to move `Vault` into `mod.rs`.
* **Cross-module private-field access is intentional coupling.** `query.rs` and
  `render.rs` read `Vault`'s private fields. That is fine at three files, but if
  the crate is ever split into multiple crates the orphan rule makes these
  `impl` blocks illegal and getters become mandatory.
* **Guidelines can go stale.** Nothing currently enforces them; they are only as
  good as the next reader.
