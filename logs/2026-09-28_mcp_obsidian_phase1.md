# 2026-09-28 — mcp_obsidian Phase 1

## 19:15 — Phase 1: CLI host + MCP server that talks to Ollama

### Objective

Turn the comment-only Rust scaffold (`main.rs`, `host.rs`, `mcp_server.rs`,
`ollama_client.rs` at the repo root) into a working CLI application that serves as
the base for the Obsidian MCP tool, and satisfy every item in `TASKS.md` Phase 1:

1. dependencies downloaded, checked for currency and stability;
2. the placeholder comment files read and implemented in their proper homes;
3. Ollama reachable at `http://localhost:11434/v1` using model `gemma4:e4b`;
4. `ServerHandler` implemented;
5. feedback written to `logs/MCP_notes.md` on how this implementation differs and
   whether either did anything better;
6. the Rust app proven able to communicate with Ollama.

### Solution

Implemented a single binary with four modules under `src/`, replacing the four
placeholder files:

* `src/vault.rs` — vault scan + in-memory index (aliases, tags, terms, link graph)
  and the four deterministic queries: `search_by_tag`, `search_by_term`,
  `get_linked_notes`, `get_note`. No I/O at query time.
* `src/mcp_server.rs` — `ObsidianServer` with `#[tool_router]` / `#[tool]` /
  `#[tool_handler]`; the same request structs and `run_*` functions feed both the
  MCP tools and the local host loop, and `schemars::schema_for!` generates the
  Ollama `tools` array from those structs.
* `src/ollama_client.rs` — HTTP client for both `/v1/chat/completions` and
  `/api/chat`, normalised into one `AssistantTurn`/`ToolCall` type.
* `src/host.rs` — the LLM loop: prompt → tool_calls → vault → answer, with tool
  failures fed back to the model as text.
* `src/main.rs` — clap CLI: `doctor`, `index`, `search`, `tools`, `ask`, `chat`,
  `serve`. Tracing is pinned to stderr so stdout stays a clean JSON-RPC channel.

Dependencies were refreshed against crates.io: `rmcp` 0.3 → **3.5.0** (the pinned
version from the MCP docs no longer compiles: `ServerInfo` was replaced by
`ServerConfig`), `reqwest` 0.12 → 0.13, `tokio` 1.46 → 1.53, `clap` gained the
`env` feature, and `walkdir` 2.5 was added for the vault scan. A sample vault was
added at `samples/vault/` for manual verification.

### Actions Performed

| # | Command | Impact |
| :- | :- | :- |
| 1 | `cargo --version`, `rustc --version` | confirmed Rust 1.98.0 / edition 2024 |
| 2 | `curl http://localhost:11434/api/version`, `/api/tags` | Ollama 0.33.1 up, 12 models, `gemma4:e4b` present and tool-capable |
| 3 | curl crates.io sparse index for 14 crates | pinned current stable versions |
| 4 | `cargo fetch` + read `rmcp-3.5.0/src/**` | verified the real 3.5 API instead of the outdated docs snippet |
| 5 | wrote `src/{vault,ollama_client,mcp_server,host,main}.rs` | full implementation |
| 6 | `rm -f main.rs host.rs mcp_server.rs ollama_client.rs` | removed the root placeholders now that they live in `src/` |
| 7 | `cargo build`, `cargo clippy` | clean: 0 errors, 0 warnings |
| 8 | created `samples/vault/` (4 notes, 11 tags, 10 links) | fixture for manual verification |
| 9 | `index`, `search tag/term/links/note`, `doctor`, `tools` | every retrieval path checked by CLI output |
| 10 | piped `initialize`/`tools/list`/`tools/call` into `serve` | MCP handler verified over stdio |
| 11 | `ask --no-tools`, `ask --trace`, `ask --api native --trace` | Ollama round trip + tool calling verified |

### Verification Output

```
$ cargo clippy
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.30s

$ mcp_obsidian --vault samples/vault doctor
ollama  version: 0.33.1 (reachable at http://localhost:11434 /api/version)
        models: 12 found
        model check: gemma4:e4b is available
vault   4 notes, 11 tags, 12 headings, 10 links (0 unresolved)
status: ready
```

```
$ mcp_obsidian --vault samples/vault ask "why does this project avoid embeddings?" --trace
tool: search_by_term {"terms":["embeddings","project"]}
tool: get_note {"note":"notes/retrieval-design.md"}
rounds: 3 | tool calls: 2
The project avoids embeddings because the structure of the vault ... is
deterministic ... The note path used for this answer is: `notes/retrieval-design.md`.
```

```
$ <5 JSON-RPC lines> | mcp_obsidian --vault samples/vault serve
# stdout (clean JSON-RPC, nothing else):
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},
 "serverInfo":{"name":"mcp_obsidian","version":"0.1.0"},
 "instructions":"Read-only retrieval over an Obsidian markdown vault."}}
{"jsonrpc":"2.0","id":2,"result":{"tools":[ 4 tools with full inputSchema ]}}
{"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"3 note(s) matched ..."}],"isError":false}}
```

```
$ cargo run -q -- ask "Reply with exactly the word: pong" --no-tools
rounds: 1 | tool calls: 0
pong                                   # 7.8 s
```

### References Used

1. https://modelcontextprotocol.io/docs/2026-07-28/develop/build-server — the Rust
   tab is the reference server used as the starting point.
2. https://docs.rs/rmcp/3.5.0/rmcp/ — current server traits, `ToolRouter`,
   `ServerConfig`, stdio transport.
3. https://github.com/ollama/ollama/blob/main/docs/api.md — `/api/chat` and the
   OpenAI-compatible `/v1/chat/completions` tool-calling payloads.

### Optional steps

* Ship a `claude_desktop_config.example.json` (snippet is in `SUMMARY.md`) so a
  desktop MCP host can be pointed at the binary without reading the source.
* Add a `--index-cache` flag to serialise the vault index and skip re-parsing when
  file mtimes are unchanged.
* Reduce scope of the binary by splitting `doctor` diagnostics behind a `--features`
  flag if it grows further.
* Move tool-failure responses to `CallToolResult::error` so MCP UIs can render them
  as failures (see `SUMMARY.md` → Suggestions).
* Fix `AGENTS.md`: it mandates Python/.venv/pip for a Rust workspace.

### Possible problems

* `AGENTS.md` conflicts with this repo's actual language (Rust, edition 2024). A
  future session following it literally would create a Python tool here.
* Frontmatter parsing is intentionally shallow (scalars, inline lists, `- item`
  lists). Nested YAML maps and block scalars are dropped silently.
* The index is rebuilt from scratch on every launch; cost grows with vault size.
* Fuzzy note-name resolution can report an ambiguous match that a weak model may not
  retry with a full path.
* `rmcp` is pre-1.0 and rewrote its server traits between 0.3 and 3.5; another
  breaking bump is plausible and would touch `mcp_server.rs`.
* Retrieval is lexical, so a question using a synonym the vault never uses returns
  nothing — the intended trade-off (no embeddings), but worth knowing.
