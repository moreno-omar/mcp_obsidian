# ROADMAP.md - Project Implementation Phases

## Phase 1: Have MCP tool talk to LLM via Ollama
- **Goals** - tool should be able to receive and send MCP requests.
- **Exit Criteria** - can establish contact to the llm via ollama, send it a request, and then receive it.

## Phase 2: Refactor
- **Goals** - to make it easier to read so I can better comprehend and extend code. Create tests to make sure refactor preserves same functionality
- **Exit Criteria** - code separated into smaller and more conside files that can be viewed mostly on 1 screen.
- **Status** - complete (2026-10-01). Largest source file is now 318 lines; the
  two monoliths (`vault.rs` 1060 → 8 files, `ollama_client.rs` 566 → 5 files)
  are split, and 32 tests pin the behaviour (see SUMMARY.md §0).

## Phase 3: Improve response
- **Goals** - to make it faster and more reliable.
- **Exit Criteria** - can answer "how does one do list comprehensions in python?" based on note in vault
- **Status** - complete (2026-10-09). `ask` no longer answers from the model's
  own knowledge: a mandatory tool protocol plus a deterministic pre-search
  (`Host::prefetch_notes`) ground every answer in the vault. Verified 5/5 runs
  against a 490-note vault (0 refusals, tool calls in every run) versus 3/5
  failures before. Output is rendered with `richrs` in a Tokyo Night theme for
  `ask`, `chat`, `doctor`, `index`, `search` and `serve` (see SUMMARY.md §0 and
  `src/ui/`).

## Phase 4: To have app available in OpenAI ChatGPT App Direcotry. Have app available as Universal MCP Server (for ollama)
- **Goals** - to have app be a compliant MCP Server, publish to Official MCP Registry. 
- **Exit Criteria** - Have app show up in OpenAI directory. Better if it can be available in Smithery and MCP.so.
- **Status** - in progress (2026-10-09). Packaging is done and verified: the
  server answers `initialize`/`tools/list`/`tools/call` over stdio; `server.json`
  (schema `2025-12-11`, `registryType: cargo`) validates against the live Official
  MCP Registry; the crate is dual-licensed and carries crates.io metadata, with
  `cargo publish --dry-run` packaging 51 files and compiling from the tarball; and
  `scripts/build_mcpb.sh` produces a schema-valid MCPB bundle for Smithery. The
  remaining steps need accounts — see [PUBLISHING.md](PUBLISHING.md) and
  [TASKS.md](TASKS.md).
- **Unblocked (2026-10-09):** the transport gap is closed. `serve --http [ADDR]`
  now serves Streamable HTTP, with `--allow-host` to admit a public hostname,
  from the same retrieval core — verified end-to-end over HTTP. What remains for
  the directory is deployment, not code: public HTTPS hosting, OAuth, and OpenAI
  organisation plus domain verification. stdio stays the default, so existing
  hosts are unaffected.
- **Renamed and prepared for crates.io (2026-10-09):** the package and executable
  are now `mcp-obsidian` (the library crate path stays `mcp_obsidian`, so no Rust
  code changed).
- **Published (2026-10-09):** `mcp-obsidian` v0.1.0 is live on crates.io —
  `cargo install mcp-obsidian`. The crate's rendered README carries the
  `mcp-name:` ownership token, so the Official MCP Registry publish is ready and
  needs only a namespace login. **Still open:** OAuth on the HTTP transport, which
  both the ChatGPT directory and Smithery's URL flow require — the full runbook,
  including the VPS hosting steps, is in [DEPLOYMENT.md](DEPLOYMENT.md). The
  Official MCP Registry, MCP.so and the ChatGPT directory are otherwise deferred
  by choice; see [TASKS.md](TASKS.md).