# SUMMARY.md — mcp_obsidian (Phase 1 complete)

Last updated: 2026-09-28 19:15

How the CLI works, why it is built this way, and what a simpler version would need.
Goal: after reading this you should be able to rebuild a smaller working version
yourself.

---

## 1. What exists now

A single Rust binary, `mcp_obsidian`, that does two jobs with one retrieval core:

1. **Local host loop** — `ask` / `chat`: prompt → Ollama → (model calls vault tools)
   → grounded answer.
2. **MCP server** — `serve`: the same four tools exposed to external MCP hosts over
   stdio.

```
src/main.rs          CLI (clap): doctor, index, search, tools, ask, chat, serve
src/vault.rs         vault scan → in-memory index → 4 deterministic queries
src/mcp_server.rs    MCP tools (#[tool]) + shared tool specs/dispatch
src/ollama_client.rs HTTP to Ollama, two wire dialects → one internal type
src/host.rs          the LLM ↔ vault loop
```

Data flow, no model in the loop:

```
vault dir ──scan──> Vault { notes, tag_index, term_index, links, backlinks }
                                                            │
              search tag|term|links|note ────────────────────┘  (pure, in-memory)
```

Data flow, with a model:

```
prompt ─> Ollama (tools[]) ─> tool_calls ─> ObsidianServer::dispatch ─> vault
   ^                                                                    │
   └────────── assistant content <── tool results (as text) <────────────┘
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

## 3. Verified output (2026-09-28)

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

## 6. Possible problems

1. **`AGENTS.md` describes the wrong language.** It mandates Python 3.10+, `.venv`
   and `pip` while this workspace is a Cargo project (`edition = "2024"`) and
   SPEC.md asks for an MCP stdio server. Following it literally would put a Python
   tool in a Rust repo. It needs either a Rust section or to be scoped to other
   projects.
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
