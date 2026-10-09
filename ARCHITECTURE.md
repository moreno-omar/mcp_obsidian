# purpose

- how code and files fit together: what to open first, what to read next, and
  where to change things.

Scope: `src/` and `tests/`. For *why* the design is this way see
[SUMMARY.md](SUMMARY.md); for what is done and what is next see
[TASKS.md](TASKS.md) and [ROADMAP.md](ROADMAP.md); for the product requirements
see [SPEC.md](SPEC.md).

## structure

### One-paragraph summary — what the project does, in plain terms.

`mcp-obsidian` is a read-only retrieval server for an Obsidian markdown vault. It
scans the vault **once** at startup into an in-memory index, then answers four
questions: by tag, by free-text term, by wikilink-graph proximity, and by exact
note. No model and no network are involved in answering. Those same four queries
are published two ways from one implementation: as MCP tools over stdio or
Streamable HTTP (`serve`) for an external host such as Claude Desktop, and as an
in-process loop (`ask` / `chat`) where a local Ollama model calls the tools
itself. There are no
embeddings — every result carries the tag, term or link that matched it, and the
same vault plus the same query always produces byte-identical output.

Model answers are grounded twice: the prompt demands a vault search before
answering, and `Host` pre-searches the vault deterministically *before* the model
is called. Terminal output is plain text when piped and Tokyo Night themed on a
terminal (`src/ui/`, the only module that writes to the user).

### Entry point — literally the file and function to open first.

1. **[src/lib.rs](src/lib.rs)** (20 lines) — the crate's public surface: five
   `pub mod` lines and nothing else. Read it first; it tells you the whole
   architecture in five lines.
2. **[src/main.rs:201](src/main.rs#L201)** — `fn run`. Argument parsing and
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
| 8 | [src/host.rs](src/host.rs) | `Host::ask` ([113](src/host.rs#L113)): the LLM ↔ tool loop that ties 6 and 7 together, plus `prefetch_notes` ([184](src/host.rs#L184)) — the pre-search that grounds answers. |
| 9 | [src/main.rs](src/main.rs) | Now the CLI reads as plumbing rather than mystery. |
| 10 | [src/ui/mod.rs](src/ui/mod.rs) + [theme.rs](src/ui/theme.rs) + [markdown.rs](src/ui/markdown.rs) + [wrap.rs](src/ui/wrap.rs) | Presentation only: how the answers above reach the terminal. Nothing in steps 1–8 imports it. |
| 11 | [tests/vault_queries.rs](tests/vault_queries.rs) | Executable documentation of vault behaviour, including every way to address a note. |

Stopping points if you only want one thing: steps 1–5 are the vault (no model, no
network); 6–8 are everything that talks to the outside world; 10 is the part you
can delete without changing a single answer.

### Module map.

```
src/
├── lib.rs              public surface only: `pub mod vault, ollama_client, mcp_server, host, ui`
├── main.rs       593   clap CLI: doctor, index, search, tools, ask, chat, serve
│
├── vault/              RETRIEVAL — pure, in-memory, no I/O at query time
│   ├── mod.rs     39   module map + defaults (DEFAULT_LIMIT / EXCERPT / BODY_CHARS)
│   ├── note.rs    82   Note, VaultStats, Hit, Direction          ← the data model
│   ├── index.rs  318   Vault struct, load/build, name resolution ← the construction
│   ├── walk.rs    69   which files/dirs a scan includes          ← fs policy
│   ├── parse.rs  282   markdown → Note: frontmatter, links, tags ← parsing
│   ├── query.rs  171   impl Vault: search_by_tag/term/linked_notes
│   ├── render.rs 112   impl Vault: render_hits/note/index  ← the payload the model sees
│   └── text.rs   163   pure string helpers (normalise, score, truncate)
│
├── mcp_server.rs 327   TOOL SURFACE — request structs, run_* fns, #[tool] methods,
│                       tool_specs() + dispatch() for in-process callers
├── ollama_client/      MODEL TRANSPORT — HTTP + both wire dialects
│   ├── mod.rs     32   module map + re-exports
│   ├── types.rs  166   ApiFlavor, Role, ChatMessage, ToolCall, AssistantTurn, ToolSpec
│   ├── client.rs 235   OllamaClient: endpoints, version, models, chat
│   ├── wire.rs   104   internal types → JSON request body
│   └── parse.rs  111   JSON response → AssistantTurn
├── host.rs       321   ORCHESTRATION — the LLM ↔ vault loop (ask/chat), incl. the
│                       deterministic pre-search that grounds answers
└── ui/                 PRESENTATION — never imported by vault/mcp_server/ollama_client
    ├── mod.rs    323   consoles (tty/NO_COLOR aware), rules, panels, tables, views, serve banner
    ├── theme.rs  169   Tokyo Night palette as semantic `richrs::Style` helpers
    ├── markdown.rs 281 markdown → styled runs (headings, code, lists, inline)
    └── wrap.rs   301   styled runs → wrapped lines (word wrap, hard wrap)

tests/
├── vault_queries.rs   22 tests over samples/vault
└── ollama_client.rs   10 tests + 1 `--ignored` live test
```

### Packaging files — outside `src/`, and none of them affect retrieval.

They exist so the binary can be handed to a marketplace. `src/` never reads them.

| File | What it is |
| :- | :- |
| [`server.json`](server.json) | Official MCP Registry entry: schema `2025-12-11`, `registryType: "cargo"`, `identifier: mcp-obsidian`, `transport: stdio`, name `io.github.moreno-omar/mcp-obsidian`. Checked by `mcp-publisher validate server.json`. |
| [`mcpb/manifest.json`](mcpb/manifest.json) | MCPB bundle manifest for Smithery / Claude Desktop: `server.type: "binary"` running `${__dirname}/server/mcp-obsidian --vault ${user_config.vault} serve`, with the vault prompted through `user_config` rather than an environment variable. |
| [`scripts/build_mcpb.sh`](scripts/build_mcpb.sh) | Builds that bundle into `dist/mcp-obsidian-<version>-<platform>.mcpb`. It refuses to run when `Cargo.toml` and the manifest disagree on the version, because a bundle whose manifest lies about the binary is the one failure that ships silently. |
| [`PUBLISHING.md`](PUBLISHING.md) | The release runbook: accounts, exact commands, and the publish order. |
| [`DEPLOYMENT.md`](DEPLOYMENT.md) | Hosting the HTTP transport on a VPS behind TLS — DNS, systemd unit, proxy streaming config, `--allow-host`, verification — and the Smithery / ChatGPT submissions, including the OAuth the code does not have yet. |
| `README.md` | Carries the `mcp-name: io.github.moreno-omar/mcp-obsidian` token in its *Marketplaces* section. The registry searches the **rendered** crate README for that literal text, and crates.io strips HTML comments, so it has to be visible prose. |
| [`Cargo.toml`](Cargo.toml) | `description`, `license`, `repository`, `keywords`, `categories`, `rust-version` (1.88 — the floor `rmcp` imposes) and an `exclude` list keeping the dev logs out of the published crate. |

### Key types/objects.

Eight types carry the design:

| Type | File | Role |
| :- | :- | :- |
| `Vault` | [vault/index.rs:25](src/vault/index.rs#L25) | All indexed state. Fields are private to `index.rs`; queries and rendering are `impl Vault` blocks in sibling files. |
| `Note` | [vault/note.rs:11](src/vault/note.rs#L11) | One parsed markdown file: path, title, tags, links, headings, frontmatter, body. |
| `Hit` | [vault/note.rs:40](src/vault/note.rs#L40) | A ranked result: index into `Vault::notes()`, score, and a human-readable *reason*. |
| `Direction` | [vault/note.rs:50](src/vault/note.rs#L50) | Which way to walk the link graph; parses the CLI strings (`outgoing`/`backlinks`/`both`). |
| `ObsidianServer` | [mcp_server.rs:124](src/mcp_server.rs#L124) | Holds `Arc<Vault>` + the rmcp `ToolRouter`. Exposes the tools to MCP hosts *and* to the local loop via `dispatch`. |
| `OllamaClient` | [ollama_client/client.rs:18](src/ollama_client/client.rs#L18) | One HTTP client for both API flavours; everything above it sees only `AssistantTurn`. |
| `Host` | [host.rs:75](src/host.rs#L75) | Wraps client + server and runs the round loop until the model stops calling tools. Also owns the pre-search ([`prefetch_notes`](src/host.rs#L184)) that grounds the prompt. |
| `Console` | [ui/mod.rs](src/ui/mod.rs) | The only place that writes to a user-visible stream. Colours off when piped, `NO_COLOR` or `serve` (stdout is JSON-RPC). `wrap_width` decides the wrap column: terminal width, or `0` when redirected. |

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
main.rs:300 run_search
  └─▶ mcp_server::run_search_by_tag | run_search_by_term
      | run_get_linked_notes | run_get_note      (defaults, validation, error text)
        └─▶ Vault::search_by_tag | search_by_term | get_linked_notes | get_note
              └─▶ Vault::rank  →  Vec<Hit>
        ◀── Vec<Hit> ──┘
        └─▶ mcp_server::render → Vault::render_hits / render_note → markdown String

`index` is the same idea, one layer shorter — but its CLI view is a
`richrs::Table` built from `Vault::stats()` + `Vault::notes()` in
`ui::print_index`, so the *payload* functions stay untouched.
```

Note the layer: the `run_*` functions own defaults, validation and error text; the
`Vault` methods own retrieval and ranking. `search` on the CLI and `tools/call`
over MCP run the *same* `run_*` function, which is why both behave identically.
The CLI then hands the markdown payload to `ui::print_markdown`.

**B. MCP server (`serve`).** [main.rs:398](src/main.rs#L398) prints the startup
panel to **stderr**, then serves the same tools one of two ways:

* **stdio** (default) — `ObsidianServer::new(vault).serve(stdio())`; stdout
  carries JSON-RPC only.
* **Streamable HTTP** (`--http [ADDR]`) —
  [main.rs:426 `run_serve_http`](src/main.rs#L426) builds one
  `StreamableHttpService` whose factory hands every session an `ObsidianServer`
  over the *same* `Arc<Vault>`, nests it at `/mcp` in an `axum` router, and
  serves it. The vault is still indexed once. HTTP mode has no auth and relies on
  [`http_config`](src/main.rs#L459), which starts from
  `StreamableHttpServerConfig::default()` — non-loopback `Host` headers rejected,
  `rmcp`'s DNS-rebinding guard — and adds any `--allow-host` names *on top*
  rather than replacing the loopback default. Public deployment still needs a
  proxy terminating TLS.

The `#[tool]` methods ([mcp_server.rs:136](src/mcp_server.rs#L136)) are thin
wrappers that log the call (`log_tool_call`, visible with `-vv`) and call the
same `run_*` functions. In both transports the banner uses
`ui::stderr_console()` and all logging is pinned to stderr in `init_tracing`. To
add a *route* (not a tool), edit the router in `run_serve_http`; to change the
transport, `Serve { http }` in the `Command` enum.

**C. Model loop (`ask`, `chat`).** [main.rs:354](src/main.rs#L354) `build_host`
wires the two halves together: `Host::new(client, Arc<ObsidianServer>)`, then
`--no-tools` / `--no-prefetch` switch off the two grounding mechanisms. The loop
([host.rs:113 `Host::ask`](src/host.rs#L113)) is:

```
prefetch = Host::prefetch_notes(prompt)        ← deterministic, no model
messages = [system(SERVER_INSTRUCTIONS + vault size + prefetch), user]
loop up to --max-rounds (default 6):
    turn = client.chat(&messages, &ObsidianServer::tool_specs())     ← ollama_client
    if turn.tool_calls.is_empty():  return AskOutcome{answer, rounds, trace, prefetch}
    push assistant tool_calls
    for each call:
        text = server.dispatch(name, args)     ← mcp_server, then vault
        push ChatMessage::tool_result(call, text)
```

`dispatch` failing is not fatal: `Host::execute` converts the error into
`ERROR: …` text the model can read and correct.
`tool_specs()` ([mcp_server.rs:191](src/mcp_server.rs#L191)) generates the model's
tool schemas from the *same* structs the MCP tools deserialize, so the two
surfaces cannot drift.

**Why the pre-search exists.** `gemma4:e4b` calls a tool on its own only about
half the time, and when it does not it answers from its own knowledge (measured
1/3 with the old prompt). The pre-search runs the prompt's subject words through
`Vault::search_by_term` first and injects the top 3 hits, so even a model that
never calls a tool answers from vault text. See SUMMARY.md §0 for the full
measurement table.

**The one round trip, end to end** (the case worth tracing once):

```
ask "…" ─▶ Host::ask
   ├─ prefetch_notes ─▶ Vault::search_by_term ▶ render_hits   (no model, ms)
   └─▶ OllamaClient::chat   (tools + pre-searched notes in the system prompt)
            ├─ wire::request_body   (messages + tools → JSON)
            ├─ POST /v1/chat/completions | /api/chat
            └─ parse::parse_*_response (JSON → AssistantTurn)
        ◀── tool_calls ─────┘
   ObsidianServer::dispatch("search_by_tag", {...})
        └─ run_search_by_tag ─▶ Vault::search_by_tag ─▶ render_hits → markdown text
        └─ ChatMessage::tool_result(...) ─▶ back to Ollama
        ◀── final content ──┘
   ui::print_answer  →  markdown → styled segments → stdout
```

### "Where to look if…"

| Task | File (and symbol) |
| :- | :- |
| Add a new MCP tool / LLM tool | [mcp_server.rs](src/mcp_server.rs) — five edits, all in one file: request struct, `run_*` fn, `#[tool]` method, `tool_specs()`, `dispatch()`. Then add the retrieval method in [vault/query.rs](src/vault/query.rs). |
| Add a CLI subcommand or flag | [main.rs](src/main.rs) — `Command` enum ([86](src/main.rs#L86)), the dispatch `match` in `fn run` ([201](src/main.rs#L201)), and a `run_*` helper. |
| Change the HTTP transport (route, host allow-list, session manager) | [main.rs:426 `run_serve_http`](src/main.rs#L426) for the router and session manager, [main.rs:459 `http_config`](src/main.rs#L459) for the `Host` allow-list. |
| Change which directories/files get scanned | [vault/walk.rs](src/vault/walk.rs) — `SKIPPED_DIRS` ([14](src/vault/walk.rs#L14)), `is_markdown`, `is_skipped_dir`. |
| Change how tags, links, headings or frontmatter are parsed | [vault/parse.rs](src/vault/parse.rs) — `split_frontmatter`, `parse_frontmatter`, `scan_body`, `extract_wikilinks` ([216](src/vault/parse.rs#L216)), `extract_inline_tags` ([248](src/vault/parse.rs#L248)). |
| Change note addressing (path/stem/alias/fuzzy) | [vault/text.rs:93 `normalize_key`](src/vault/text.rs#L93) and [vault/index.rs:165 `resolve_indices`](src/vault/index.rs#L165) / `resolve_one`. |
| Change how results are ranked or scored | [vault/query.rs:141 `combine_tag_scores`](src/vault/query.rs#L141), [vault/text.rs:16 `score_terms`](src/vault/text.rs#L16), [vault/index.rs:269 `rank`](src/vault/index.rs#L269). |
| Change what the model/host sees (the markdown payload) | [vault/render.rs](src/vault/render.rs) — `render_hits`, `render_note`, `render_index`; truncation markers live in [vault/text.rs:131](src/vault/text.rs#L131). |
| Change a default (limit, excerpt size, body budget) | [vault/mod.rs:35](src/vault/mod.rs#L35) for the constants, [mcp_server.rs:41](src/mcp_server.rs#L41) for their serde default hooks. |
| Change the system prompt / instructions | [mcp_server.rs:30 `SERVER_INSTRUCTIONS`](src/mcp_server.rs#L30) — shared by the MCP `instructions` field and [host.rs:204 `system_prompt`](src/host.rs#L204). The MCP `#[tool_handler]` attribute needs a literal, so its short form is kept in sync by hand. |
| Change how an answer is grounded | [host.rs:184 `prefetch_notes`](src/host.rs#L184) (query terms + 3 notes × 800 chars) and [host.rs:280 `search_terms`](src/host.rs#L280) (`STOPWORDS`, `PREFETCH_*` constants at the top of the file). |
| Change the terminal look (colours, accents) | [ui/theme.rs](src/ui/theme.rs) — one function per role; edit the RGB values in `rgb()` calls. |
| Change how a model answer is rendered | [ui/markdown.rs](src/ui/markdown.rs) — `render` (block level), `inline_runs` / `take_inline_token` (inline level). |
| Change how long lines are broken | [ui/wrap.rs](src/ui/wrap.rs) — `word_wrap` (prose), `hard_wrap` (code), `break_at_space`, `collapse_blank_lines`. Width `0` means "do not wrap". |
| Add or restyle a CLI view (panel, table, rule) | [ui/mod.rs](src/ui/mod.rs) — `print_panel`, `print_index`, `print_search`, `print_trace`; called from [main.rs](src/main.rs) and [host.rs:223 `chat_loop`](src/host.rs#L223). |
| Change when colour is emitted | [ui/mod.rs](src/ui/mod.rs) — `plain_when_redirected` / `force_colours` (`NO_COLOR`, `CLICOLOR_FORCE`, `FORCE_COLOR`). |
| Change endpoints, timeouts, or the doctor probe | [ollama_client/client.rs](src/ollama_client/client.rs) — `new` ([28](src/ollama_client/client.rs#L28)), `*_url()`, `version`, `models`. |
| Change how a request is encoded (per dialect) | [ollama_client/wire.rs](src/ollama_client/wire.rs) — `request_body` ([12](src/ollama_client/wire.rs#L12)), `message_to_native`, `message_to_openai`. |
| Change how a reply is decoded | [ollama_client/parse.rs](src/ollama_client/parse.rs) — `parse_native_response` ([14](src/ollama_client/parse.rs#L14)), `parse_openai_response` ([26](src/ollama_client/parse.rs#L26)), `parse_tool_calls` ([55](src/ollama_client/parse.rs#L55)). |
| Change the loop (round cap, tracing, prompt assembly) | [host.rs](src/host.rs) — `DEFAULT_MAX_ROUNDS` ([32](src/host.rs#L32)), `ask`, `execute`, `chat_loop`. |
| Change logging verbosity/shape | [main.rs:580 `init_tracing`](src/main.rs#L580) — stderr only, by design. |
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
