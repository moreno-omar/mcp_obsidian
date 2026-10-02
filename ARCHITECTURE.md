# purpose

- how code and files fit together: what to open first, what to read next, and
  where to change things.

Scope: `src/` and `tests/`. For *why* the design is this way see
[SUMMARY.md](SUMMARY.md); for what is done and what is next see
[TASKS.md](TASKS.md) and [ROADMAP.md](ROADMAP.md); for the product requirements
see [SPEC.md](SPEC.md).

## structure

### One-paragraph summary — what the project does, in plain terms.

`mcp_obsidian` is a read-only retrieval server for an Obsidian markdown vault. It
scans the vault **once** at startup into an in-memory index, then answers four
questions: by tag, by free-text term, by wikilink-graph proximity, and by exact
note. No model and no network are involved in answering. Those same four queries
are published two ways from one implementation: as MCP tools over stdio
(`serve`) for an external host such as Claude Desktop, and as an in-process loop
(`ask` / `chat`) where a local Ollama model calls the tools itself. There are no
embeddings — every result carries the tag, term or link that matched it, and the
same vault plus the same query always produces byte-identical output.

### Entry point — literally the file and function to open first.

1. **[src/lib.rs](src/lib.rs)** (18 lines) — the crate's public surface: four
   `pub mod` lines and nothing else. Read it first; it tells you the whole
   architecture in five lines.
2. **[src/main.rs:162](src/main.rs#L162)** — `fn main`. Argument parsing and
   dispatch only; no logic lives here.

Everything else hangs off those two.

### Reading order — a numbered path.

Follow this and each step only needs the one before it:

| # | Read | Why here |
| :- | :- | :- |
| 1 | [src/vault/note.rs](src/vault/note.rs) | The data model: `Note`, `Hit`, `VaultStats`, `Direction`. Nothing else makes sense until these four are clear. |
| 2 | [src/vault/index.rs](src/vault/index.rs) | `Vault::load` ([45](src/vault/index.rs#L45)) → `build` ([71](src/vault/index.rs#L71)) → `index_notes` → `resolve_links`. This *is* the architecture: one pass builds every index. |
| 3 | [src/vault/walk.rs](src/vault/walk.rs) + [src/vault/parse.rs](src/vault/parse.rs) | The two things step 2 calls: which files are read, and how one file becomes a `Note`. |
| 4 | [src/vault/query.rs](src/vault/query.rs) | The four public queries and their ranking. Read alongside [src/vault/text.rs](src/vault/text.rs) (`score_terms`, `normalize_*`). |
| 5 | [src/vault/render.rs](src/vault/render.rs) | Hits and notes → markdown. This is where the LLM-visible text is decided. |
| 6 | [src/mcp_server.rs](src/mcp_server.rs) | The MCP/wire layer: request structs → `run_*` functions → `#[tool]` methods, `tool_specs()` and `dispatch()`. |
| 7 | [src/ollama_client/types.rs](src/ollama_client/types.rs) → [client.rs](src/ollama_client/client.rs) → [wire.rs](src/ollama_client/wire.rs) → [parse.rs](src/ollama_client/parse.rs) | The HTTP layer, in the order the data travels. |
| 8 | [src/host.rs](src/host.rs) | `Host::ask` ([63](src/host.rs#L63)): the LLM ↔ tool loop that ties 6 and 7 together. |
| 9 | [src/main.rs](src/main.rs) | Now the CLI reads as plumbing rather than mystery. |
| 10 | [tests/vault_queries.rs](tests/vault_queries.rs) | Executable documentation of vault behaviour, including every way to address a note. |

Stopping points if you only want one thing: steps 1–5 are the vault (no model, no
network); 6–8 are everything that talks to the outside world.

### Module map.

```
src/
├── lib.rs              public surface only: `pub mod vault, ollama_client, mcp_server, host`
├── main.rs       414   clap CLI: doctor, index, search, tools, ask, chat, serve
│
├── vault/              RETRIEVAL — pure, in-memory, no I/O at query time
│   ├── mod.rs     39   module map + defaults (DEFAULT_LIMIT / EXCERPT / BODY_CHARS)
│   ├── note.rs    82   Note, VaultStats, Hit, Direction          ← the data model
│   ├── index.rs  318   Vault struct, load/build, name resolution ← the construction
│   ├── walk.rs    69   which files/dirs a scan includes          ← fs policy
│   ├── parse.rs  282   markdown → Note: frontmatter, links, tags ← parsing
│   ├── query.rs  171   impl Vault: search_by_tag/term/linked_notes
│   ├── render.rs 112   impl Vault: render_hits/note/index
│   └── text.rs   163   pure string helpers (normalise, score, truncate)
│
├── mcp_server.rs 308   TOOL SURFACE — request structs, run_* fns, #[tool] methods,
│                       tool_specs() + dispatch() for in-process callers
├── ollama_client/      MODEL TRANSPORT — HTTP + both wire dialects
│   ├── mod.rs     32   module map + re-exports
│   ├── types.rs  166   ApiFlavor, Role, ChatMessage, ToolCall, AssistantTurn, ToolSpec
│   ├── client.rs 235   OllamaClient: endpoints, version, models, chat
│   ├── wire.rs   104   internal types → JSON request body
│   └── parse.rs  111   JSON response → AssistantTurn
└── host.rs       184   ORCHESTRATION — the LLM ↔ vault loop (ask/chat)

tests/
├── vault_queries.rs   22 tests over samples/vault
└── ollama_client.rs   10 tests + 1 `--ignored` live test
```

### Key types/objects.

Seven types carry the design:

| Type | File | Role |
| :- | :- | :- |
| `Vault` | [vault/index.rs:25](src/vault/index.rs#L25) | All indexed state. Fields are private to `index.rs`; queries and rendering are `impl Vault` blocks in sibling files. |
| `Note` | [vault/note.rs:11](src/vault/note.rs#L11) | One parsed markdown file: path, title, tags, links, headings, frontmatter, body. |
| `Hit` | [vault/note.rs:40](src/vault/note.rs#L40) | A ranked result: index into `Vault::notes()`, score, and a human-readable *reason*. |
| `Direction` | [vault/note.rs:50](src/vault/note.rs#L50) | Which way to walk the link graph; parses the CLI strings (`outgoing`/`backlinks`/`both`). |
| `ObsidianServer` | [mcp_server.rs:118](src/mcp_server.rs#L118) | Holds `Arc<Vault>` + the rmcp `ToolRouter`. Exposes the tools to MCP hosts *and* to the local loop via `dispatch`. |
| `OllamaClient` | [ollama_client/client.rs:18](src/ollama_client/client.rs#L18) | One HTTP client for both API flavours; everything above it sees only `AssistantTurn`. |
| `Host` | [host.rs:33](src/host.rs#L33) | Wraps client + server and runs the round loop until the model stops calling tools. |

```
vault dir
   │  walk::markdown_files → parse::parse_note → Note
   ▼
Vault::load → build → { notes, aliases, tag_index, term_index,
                        outgoing, incoming, unresolved }
   │
   │  query.rs    search_by_tag | search_by_term | get_linked_notes | get_note
   ▼
Vec<Hit>
   │  render.rs   render_hits | render_note | render_index
   ▼
markdown String
   ▲
   │ produced by
mcp_server.rs::run_*    ← the single source of truth for defaults,
   │                      validation and error text
   ├── main.rs `index` / `search`          (CLI, nothing else involved)
   ├── #[tool] methods                     (MCP host, over stdio)
   └── ObsidianServer::dispatch            (host::Host::ask → ollama_client::chat)
```

### Data/control flow.

There are exactly three paths through the code. Every CLI subcommand is one of them.

**A. Vault-only (`index`, `search`, and the MCP tools).** No model, no network:

```
main.rs:227 run_search
  └─▶ mcp_server::run_search_by_tag | run_search_by_term
      | run_get_linked_notes | run_get_note      (defaults, validation, error text)
        └─▶ Vault::search_by_tag | search_by_term | get_linked_notes | get_note
              └─▶ Vault::rank  →  Vec<Hit>
        ◀── Vec<Hit> ──┘
        └─▶ mcp_server::render → Vault::render_hits / render_note → markdown String

main.rs:169 `index` is the same idea, one layer shorter:
  └─▶ Vault::render_index(limit)
```

Note the layer: the `run_*` functions own defaults, validation and error text; the
`Vault` methods own retrieval and ranking. `search` on the CLI and `tools/call`
over MCP run the *same* `run_*` function, which is why both behave identically.

**B. MCP server (`serve`).** [main.rs:315](src/main.rs#L315) starts
`ObsidianServer::new(vault).serve(stdio())`. The `#[tool]` methods
([mcp_server.rs:136](src/mcp_server.rs#L136)) are thin wrappers that call the same
`run_*` functions. stdout carries JSON-RPC only — all logging is pinned to stderr
in `init_tracing` ([main.rs:401](src/main.rs#L401)).

**C. Model loop (`ask`, `chat`).** [main.rs:281](src/main.rs#L281) `build_host`
wires the two halves together: `Host::new(client, Arc<ObsidianServer>)`. Then
[host.rs:63 `Host::ask`](src/host.rs#L63):

```
messages = [system, user]
loop up to --max-rounds (default 6):
    turn = client.chat(&messages, &ObsidianServer::tool_specs())     ← ollama_client
    if turn.tool_calls.is_empty():  return AskOutcome{answer, rounds, trace}
    push assistant tool_calls
    for each call:
        text = server.dispatch(name, args)     ← mcp_server, then vault
        push ChatMessage::tool_result(call, text)
```

`dispatch` failing is not fatal: `Host::execute` ([114](src/host.rs#L114))
converts the error into `ERROR: …` text the model can read and correct.
`tool_specs()` ([mcp_server.rs:177](src/mcp_server.rs#L177)) generates the model's
tool schemas from the *same* structs the MCP tools deserialize, so the two
surfaces cannot drift.

**The one round trip, end to end** (the case worth tracing once):

```
ask "…" ─▶ Host::ask ─▶ OllamaClient::chat
                            ├─ wire::request_body   (messages + tools → JSON)
                            ├─ POST /v1/chat/completions | /api/chat
                            └─ parse::parse_*_response (JSON → AssistantTurn)
        ◀── tool_calls ─────┘
   ObsidianServer::dispatch("search_by_tag", {...})
        └─ run_search_by_tag ─▶ Vault::search_by_tag ─▶ render_hits → markdown text
        └─ ChatMessage::tool_result(...) ─▶ back to Ollama
        ◀── final content ──┘
```

### "Where to look if…"

| Task | File (and symbol) |
| :- | :- |
| Add a new MCP tool / LLM tool | [mcp_server.rs](src/mcp_server.rs) — five edits, all in one file: request struct, `run_*` fn, `#[tool]` method, `tool_specs()`, `dispatch()`. Then add the retrieval method in [vault/query.rs](src/vault/query.rs). |
| Add a CLI subcommand or flag | [main.rs](src/main.rs) — `Command` enum ([82](src/main.rs#L82)), the dispatch `match` in `fn main` ([167](src/main.rs#L167)), and a `run_*` helper. |
| Change which directories/files get scanned | [vault/walk.rs](src/vault/walk.rs) — `SKIPPED_DIRS` ([14](src/vault/walk.rs#L14)), `is_markdown`, `is_skipped_dir`. |
| Change how tags, links, headings or frontmatter are parsed | [vault/parse.rs](src/vault/parse.rs) — `split_frontmatter`, `parse_frontmatter`, `scan_body`, `extract_wikilinks` ([216](src/vault/parse.rs#L216)), `extract_inline_tags` ([248](src/vault/parse.rs#L248)). |
| Change note addressing (path/stem/alias/fuzzy) | [vault/text.rs:93 `normalize_key`](src/vault/text.rs#L93) and [vault/index.rs:165 `resolve_indices`](src/vault/index.rs#L165) / `resolve_one`. |
| Change how results are ranked or scored | [vault/query.rs:141 `combine_tag_scores`](src/vault/query.rs#L141), [vault/text.rs:16 `score_terms`](src/vault/text.rs#L16), [vault/index.rs:269 `rank`](src/vault/index.rs#L269). |
| Change what the model/host sees (the markdown payload) | [vault/render.rs](src/vault/render.rs) — `render_hits`, `render_note`, `render_index`; truncation markers live in [vault/text.rs:131](src/vault/text.rs#L131). |
| Change a default (limit, excerpt size, body budget) | [vault/mod.rs:35](src/vault/mod.rs#L35) for the constants, [mcp_server.rs:35](src/mcp_server.rs#L35) for their serde default hooks. |
| Change the system prompt / instructions | [mcp_server.rs:27 `SERVER_INSTRUCTIONS`](src/mcp_server.rs#L27) — shared by the MCP `instructions` field and [host.rs:123 `system_prompt`](src/host.rs#L123). |
| Change endpoints, timeouts, or the doctor probe | [ollama_client/client.rs](src/ollama_client/client.rs) — `new` ([28](src/ollama_client/client.rs#L28)), `*_url()`, `version`, `models`. |
| Change how a request is encoded (per dialect) | [ollama_client/wire.rs](src/ollama_client/wire.rs) — `request_body` ([12](src/ollama_client/wire.rs#L12)), `message_to_native`, `message_to_openai`. |
| Change how a reply is decoded | [ollama_client/parse.rs](src/ollama_client/parse.rs) — `parse_native_response` ([14](src/ollama_client/parse.rs#L14)), `parse_openai_response` ([26](src/ollama_client/parse.rs#L26)), `parse_tool_calls` ([55](src/ollama_client/parse.rs#L55)). |
| Change the loop (round cap, tracing, prompt assembly) | [host.rs](src/host.rs) — `DEFAULT_MAX_ROUNDS` ([18](src/host.rs#L18)), `ask`, `execute`, `chat_loop`. |
| Change logging verbosity/shape | [main.rs:401 `init_tracing`](src/main.rs#L401) — stderr only, by design. |
| Add or change tests | [tests/vault_queries.rs](tests/vault_queries.rs), [tests/ollama_client.rs](tests/ollama_client.rs); fixture at [samples/vault/](samples/vault). |

### Glossary.

| Term | Meaning here |
| :- | :- |
| **vault** | A directory of markdown notes; the read-only input to everything. |
| **note** | One `.md` file, parsed into a [`Note`](src/vault/note.rs#L11). `rel_path` is vault-relative with `/` separators. |
| **frontmatter** | The optional `---` block at the top of a note, flattened into `key -> Vec<String>` ([parse.rs:125](src/vault/parse.rs#L125)). Deliberately shallow: no nested maps or anchors. |
| **tag** | Normalised to lowercase, no `#`, inner spaces → `-`. A query for `project` also matches the nested tag `project/alpha` (`notes_with_tag`). |
| **wikilink / backlink** | `[[Target]]` written *in* a note is an outgoing link; the reverse edge is a backlink. Both are resolved to note indices in `resolve_links`. |
| **alias** | Any name a note answers to: its path, its file stem, or a frontmatter `aliases:` entry. Built in [parse.rs:64 `note_aliases`](src/vault/parse.rs#L64). |
| **term** | A lowercased word of ≥2 characters, from title/tags/headings/frontmatter/body. The inverted index of terms is what makes term search avoid full scans. |
| **hit** | A scored, ranked result with a `reason` string explaining *why* it matched — the deterministic, non-embedding part of the design. |
| **excerpt vs body budget** | Search hits carry a short excerpt (`DEFAULT_EXCERPT_CHARS`); `get_note` returns a whole note truncated to `DEFAULT_BODY_CHARS`. `0` means "no limit". |
| **dialect / flavour** | Which HTTP API shape Ollama is spoken to: `openai` (`/v1/chat/completions`, arguments as a JSON string) or `native` (`/api/chat`, arguments as an object). |
| **tool spec** | A tool definition sent to the model, generated from the MCP request structs by `tool_specs()`. |
| **host loop** | `ask`/`chat`: prompt → model → tool calls → vault → tool results → final answer, capped by `--max-rounds`. Not an MCP client connection — the tool layer is called in-process. |
| **unresolved link** | A wikilink target with no matching note. Counted in `stats()` and reported by `doctor` as link rot; it never fails a query. |
