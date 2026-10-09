# Rust Module & Visibility Guidelines

How `src/vault/` is organised, why `mod.rs` looks like it does, and how to
apply the same patterns elsewhere in this crate.

Grounded in the real files: [`src/vault/mod.rs`](src/vault/mod.rs),
[`src/vault/index.rs`](src/vault/index.rs), [`src/vault/note.rs`](src/vault/note.rs),
[`src/lib.rs`](src/lib.rs), [`src/main.rs`](src/main.rs).

---

## 0. TL;DR

| Question | Answer |
| :- | :- |
| Does `pub use index::Vault;` use the *implementation* from `index.rs`? | No. It re-exports the **name**. The implementation already lives in `index.rs` and never moves. |
| Where is `Vault` defined? | `src/vault/index.rs:25` (`pub struct Vault`). |
| Why can't I write `vault::index::Vault` outside the crate? | `mod index;` is private; only `vault::Vault` is public. |
| Can `mod.rs` define a `pub struct`? | Yes. `mod.rs` **is** the module body; it may hold any item. |
| Can `impl Vault` live in a different file from the struct? | Yes — same crate, any module. Rust merges all `impl` blocks into one type. |
| What is `pub(crate)` for? | "Visible anywhere in this crate, but not to other crates." Used for internal helpers such as `parse_note`. |

---

## 1. How the module system works

### Files become modules

`mod name;` tells the compiler *"there is a child module called `name`; go find
its file"*. The compiler looks in exactly two places:

| Declaration | File locations accepted |
| :- | :- |
| `mod foo;` | `src/foo.rs` **or** `src/foo/mod.rs` (not both) |
| `mod a;` inside `src/foo/mod.rs` | `src/foo/a.rs` **or** `src/foo/a/mod.rs` |
| `mod a;` inside `src/foo.rs` | `src/foo/a.rs` **or** `src/foo/a/mod.rs` |

Both spellings are equivalent; `foo.rs` + `foo/` is the Rust 2018+ preference,
`mod.rs` is the older convention. This project uses `mod.rs`, which is why
[`src/vault/mod.rs`](src/vault/mod.rs) and the `src/vault/` directory coexist.

### `mod.rs` is an ordinary module body

This is the single most misunderstood point. `mod.rs` is **not** a manifest or a
declaration-only file — it *is* the module. It can contain structs, enums, `impl`
blocks, `fn`s, `const`s, `use` and re-exports, exactly like any other file.

```rust
// src/foo/mod.rs — all of this is legal
mod a;
mod b;

pub struct Foo { pub n: u32 }        // type defined here
impl Foo { pub fn new() -> Self { Foo { n: 1 } } }

pub use a::Bar;                      // re-export from a child
pub const DEFAULT_LIMIT: usize = 10;
```

Children reach it as `super::Foo` (because `super` = the parent module):

```rust
// src/foo/a.rs
impl super::Foo { pub fn triple(&self) -> u32 { self.n * 3 } }
```

So a note saying "`mod.rs` can define `pub struct Foo`" is **correct**.

### The tree in this crate

```
mcp_obsidian (lib crate, src/lib.rs)
├── vault            src/lib.rs:18        pub mod vault;
│   ├── index        src/vault/mod.rs:23  mod index;        (private)
│   ├── note         src/vault/mod.rs:24  mod note;         (private)
│   ├── parse        src/vault/mod.rs:25  mod parse;        (private)
│   ├── query        src/vault/mod.rs:26  mod query;        (private)
│   ├── render       src/vault/mod.rs:27  mod render;       (private)
│   ├── text         src/vault/mod.rs:28  mod text;         (private)
│   └── walk         src/vault/mod.rs:29  mod walk;         (private)
├── mcp_server       src/lib.rs:16        pub mod mcp_server;
├── ollama_client    src/lib.rs:17        pub mod ollama_client;
└── host             src/lib.rs:15        pub mod host;
```

`src/main.rs` is a *separate* crate (a binary) that depends on this one, so it
imports through the crate name: `use mcp_obsidian::vault::...`.

### Visibility defaults

Everything is **private to its parent module** unless marked otherwise. A private
child module is a one-way door: the parent can name it (`index::Vault`), but
nobody outside `vault` can, even if the item inside is `pub`.

```rust
mod index;              // private module  → path `vault::index` is not public
pub struct Vault        // pub item in a private module → reachable only as vault::index::Vault
pub use index::Vault;   // fix: publish it as `vault::Vault`
```

---

## 2. Visibility levels

| Keyword | Meaning |
| :- | :- |
| *(none)* | Private to the containing module and its descendants. |
| `pub` | Public all the way out — from any crate that depends on us. |
| `pub(crate)` | Public anywhere inside **this crate**, invisible to dependents. |
| `pub(super)` | Visible to the parent module only. |
| `pub(in path)` | Visible within an explicit ancestor module path. |

### The `pub(crate)` pattern in this repo

`vault`'s internal helpers — in [`src/vault/parse.rs`](src/vault/parse.rs) and
[`src/vault/walk.rs`](src/vault/walk.rs) — are `pub(crate)` so sibling modules
can call them, while downstream users (and the `main` binary) cannot:

```rust
// src/vault/parse.rs:29
pub(crate) fn parse_note(rel_path: &str, raw: &str) -> Note { /* ... */ }

// src/vault/parse.rs:64
pub(crate) fn note_aliases(note: &Note) -> Vec<String> { /* ... */ }

// src/vault/walk.rs:29
pub(crate) fn markdown_files(root: &Path) -> Result<Vec<PathBuf>> { /* ... */ }

// src/vault/walk.rs:47
pub(crate) fn read_note(path: &Path) -> Result<String> { /* ... */ }
```

and consumed by the only caller, `Vault::load`:

```rust
// src/vault/index.rs:19-21
use super::parse::{note_aliases, parse_note};
use super::text::{collect_terms, normalize_key};
use super::walk::{markdown_files, read_note};
```

Why `pub(crate)` and not `pub`:

* It is **self-documenting**: `pub(crate)` says "internal seam, may change".
* It prevents accidental API growth — a helper cannot leak into the public
  surface just because someone forgot to think about it.
* It still allows free splitting and merging of internal files without breaking
  any external `use`.

> Because `mod parse;` is private, `pub(crate)` here is roughly "crate-visible";
> using `pub` would also work but would over-promise. Prefer the narrowest
> visibility that compiles.

Truly file-local helpers stay plain private — see `flush_term` and
`has_phrase_hit` in [`src/vault/text.rs`](src/vault/text.rs).

---

## 3. What `pub use index::Vault;` actually does

```rust
// src/vault/mod.rs:23 and :31
mod index;              // compile src/vault/index.rs as child module `index`
pub use index::Vault;   // re-export that item at THIS module's path
```

Step by step:

1. `mod index;` compiles [`src/vault/index.rs`](src/vault/index.rs) and makes
   `index` a child of `vault`. The item `Vault` is born at path
   `crate::vault::index::Vault`.
2. `pub struct Vault` in `index.rs:25` makes the item `pub`, but the *path*
   `crate::vault::index` is private, so the item is unreachable from outside.
3. `pub use index::Vault;` binds the name `Vault` **in the `vault` namespace**
   and marks that binding public. The canonical public path becomes
   `crate::vault::Vault`.

Key point: it is a **re-export** (`use` + `pub`), not a definition and not a
move. `Vault` is still defined in `index.rs`; the `use` only decides which path
you are allowed to name it by.

```rust
use mcp_obsidian::vault::index::Vault;  // ✗ error: module `index` is private
use mcp_obsidian::vault::Vault;         // ✓ the public door
```

### The benefit — `src/main.rs:36`

Without the re-export, the binary could not name `Vault` at all. With it, the
call site is short and stable:

```rust
// src/main.rs:36
use mcp_obsidian::vault::{DEFAULT_BODY_CHARS, DEFAULT_LIMIT, Vault};
```

```rust
// src/mcp_server.rs:24
use crate::vault::{DEFAULT_BODY_CHARS, DEFAULT_EXCERPT_CHARS, DEFAULT_LIMIT, Direction, Vault};
```

One import line pulls the type, the enum and the three tuning constants from a
flat namespace — callers never learn that `Vault` lives in `index.rs`,
`Direction` in `note.rs` and the constants in `mod.rs` itself.

Concrete payoff: [Phase 2 of the roadmap](logs/2026-10-01_mcp_obsidian_phase2_refactor.md)
split a 1060-line `src/vault.rs` into eight files. Because callers only ever
used `vault::Vault`, **no call site changed**. The file layout is an
implementation detail; the facade is the contract.

---

## 4. `impl` blocks in many files become one type

Rust has no partial classes, but it *does* allow inherent `impl` blocks for a
crate-local type in **any module of the same crate**. The compiler collects them
all and attaches them to the single type.

| File | Block | Contributes |
| :- | :- | :- |
| [`src/vault/index.rs`](src/vault/index.rs) | `impl Vault` (line 43) | `load`, `stats`, `get_note`, name resolution |
| [`src/vault/query.rs`](src/vault/query.rs) | `impl Vault` (line 14) | `search_by_tag`, `search_by_term`, `get_linked_notes` |
| [`src/vault/render.rs`](src/vault/render.rs) | `impl Vault` (line 13) | `render_*` markdown output |

```rust
// these live in three different files, but compile to ONE type
impl Vault { /* index.rs   */ }
impl Vault { /* query.rs   */ }
impl Vault { /* render.rs  */ }
```

All of them are reachable through the single re-exported name:

```rust
let vault = Vault::load(&root)?;   // defined in index.rs
let hits  = vault.search_by_tag(&["writing".into()], true, 10); // defined in query.rs
let text  = vault.render_note(&note, 4000);                    // defined in render.rs
```

Two consequences to remember:

1. **Methods travel with the type, not the path.** No `pub use` is needed for
   `search_by_tag` to be callable — re-exporting `Vault` is enough.
2. **Private fields are shared within the crate.** `query.rs` and `render.rs`
   read `Vault`'s private fields (`notes`, `tag_index`, `term_index`, …) because
   field privacy in Rust is *module-of-definition* based, and all three files are
   in the same crate. This is what makes the concern-split possible without
   getters everywhere.

The orphan rule (`impl` only where the trait or the type is local) applies to
**crates**, not modules. Splitting `impl Vault` across modules is always fine
inside this crate; it would be rejected in a different crate.

---

## 5. The facade pattern

`vault` is a **facade** module: a wide internal implementation behind a narrow,
stable front door.

```
   callers (main.rs, mcp_server.rs)
                │  use mcp_obsidian::vault::{Vault, Direction, DEFAULT_LIMIT}
                ▼
   ┌──────────────────────────────────────────────┐
   │ src/vault/mod.rs   — the facade               │
   │   mod index; mod note; mod parse; ...  (hidden)│
   │   pub use index::Vault;                (export)│
   │   pub use note::{Direction, Hit, Note, VaultStats}; │
   │   pub const DEFAULT_LIMIT / _EXCERPT_CHARS / _BODY_CHARS │
   └──────────────────────────────────────────────┘
        │            │           │          │
     index.rs     note.rs    query.rs   render.rs   (private)
```

Properties this buys:

* **Encapsulation** — `parse`, `walk` and `text` can be rewritten or merged
  freely; they are not part of any contract.
* **A stable public API** — the public surface is exactly the two `pub use`
  lines plus three constants in [`src/vault/mod.rs`](src/vault/mod.rs), not
  eight files.
* **Readable call sites** — `vault::Vault`, not `vault::index::Vault`.
* **Layered dependencies** — arrows point *into* `index.rs`:

```
  query.rs / render.rs   ── impl Vault, read private index state
          ▲
  index.rs               ── owns Vault + its fields; calls the helpers
          ▲
  parse.rs / walk.rs / text.rs ── free functions, no Vault state
                                  (parse/text pure; walk = scan policy)
          ▲
  note.rs                ── plain data types + impls for their own enums
```

---

## 6. Rule of thumb: `impl` block or free function?

| Signal | Put it as | Example here |
| :- | :- | :- |
| Needs `&self` / `&mut self` / `Self` | `impl` method | `Vault::search_by_tag` |
| Pure value → value mapping, no vault state | free `fn` in its own module | `parse_note(rel_path, raw) -> Note` |
| Defined for a type this crate owns | `impl` next to the type | `impl FromStr for Direction` in `note.rs` |
| Shared by several modules, stateless | free `fn`, `pub(crate)` | `collect_terms`, `truncate_chars` in `text.rs` |

One more: **one concern per `impl` file.** Construction in `index.rs`, retrieval
in `query.rs`, formatting in `render.rs`. If a new method needs `&self` but fits
none of the three, that is a signal for a fourth file — not for growing
`index.rs`.

---

## 7. Best practices checklist

**Module layout**

- [ ] `mod.rs` holds declarations, re-exports and shared constants; keep it
      short and readable (this one is 40 lines).
- [ ] Treat `mod.rs` as a normal module — a type defined there is fine.
- [ ] Group children by **concern**, not by technical category.
- [ ] Keep the file's doc comment (`//!`) as a module map, like the table at the
      top of [`src/vault/mod.rs`](src/vault/mod.rs).

**Visibility**

- [ ] Start private. Widen only when the compiler forces you to.
- [ ] `pub(crate)` for internal seams (`parse_note`, `markdown_files`).
- [ ] Plain private for truly file-local helpers (`flush_term`).
- [ ] Re-export a **flat, curated** face; never `pub use` an entire internal
      module wholesale.
- [ ] Publish constants next to the code that documents them (`DEFAULT_LIMIT`).

**Impl blocks**

- [ ] Keep the struct definition and its private fields in one place.
- [ ] Split method groups across files by concern; annotate with a `//!` line.
- [ ] Do not add getters purely "for other modules" — same-crate sibling modules
      already see private fields.

**Change safety**

- [ ] Renaming or merging a private file must not touch call sites. If it does,
      something leaked out of the facade.
- [ ] Verify with `cargo build` and `cargo doc` (doc links between modules are a
      good facade check).

---

## 8. Short examples

**(a) Definition, re-export, use**

```rust
// src/vault/index.rs
pub struct Vault { /* fields stay private */ }
impl Vault { pub fn load(root: impl AsRef<Path>) -> Result<Self> { /* ... */ } }

// src/vault/mod.rs
mod index;
pub use index::Vault;          // canonical path: crate::vault::Vault

// src/main.rs
use mcp_obsidian::vault::Vault;
let vault = Vault::load("samples/vault")?;
```

**(b) Adding a method from another file**

```rust
// src/vault/query.rs
use super::index::Vault;      // `super` = the parent module
impl Vault {
    pub fn search_by_tag(&self, tags: &[String], match_all: bool, limit: usize) -> Vec<Hit> {
        let _ = &self.tag_index;   // private field, same crate → visible
        todo!()
    }
}
```

**(c) `pub(crate)` helper used by the index**

```rust
// src/vault/parse.rs
pub(crate) fn parse_note(rel_path: &str, raw: &str) -> Note { /* ... */ }

// src/vault/index.rs
use super::parse::parse_note;
let note = parse_note(&rel_path, &raw);
```

**(d) The facade makes refactors free**

```rust
// before split:  struct Vault in src/vault.rs
// after split:   struct Vault in src/vault/index.rs + pub use
//
// call sites, unchanged:
use mcp_obsidian::vault::{DEFAULT_LIMIT, Vault};   // src/main.rs:36
```

---

## 9. Common mistakes

| Mistake | Symptom | Fix |
| :- | :- | :- |
| Adding `pub` to every item | Public API grows silently; semver pain | Start private, widen on demand |
| Forgetting `pub use` in the facade | `error[E0603]: module 'index' is private` | Re-export the item, or make the module public |
| Duplicating `impl Vault` in two files for the same method | `error[E0592]: duplicate definitions with name ...` | One method, one file |
| Assuming `pub use` moves or re-implements the type | Confusion about "where the implementation lives" | The `struct`/`impl` declarations decide that; `use` only sets the path |
| Using `pub(crate)` for something `main.rs` needs | `error[E0603]` at the binary | `main.rs` is another crate → use `pub` |
| Re-exporting internal helpers "just in case" | Facade leaks; internals become a contract | Re-export only the curated surface |

---

## 10. References

- The Rust Programming Language — *Modules and Paths*: <https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html>
- The Rust Reference — *Use declarations*, *Visibility and privacy*: <https://doc.rust-lang.org/reference/visibility-and-privacy.html>
- Rust API Guidelines — *Documentation* / *Future proofing*: <https://rust-lang.github.io/api-guidelines/>

---

*Last updated: 2026-10-02. Verified by compiling a scratch crate exercising
`mod.rs` items, `impl` in a child module, and a re-export.*
