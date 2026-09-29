# TASKS.md - Phase 1

> Completed 2026-09-28. Full log in `logs/2026-09-28_mcp_obsidian_phase1.md`,
> implementation comparison in `logs/MCP_notes.md`, how-it-works in `SUMMARY.md`.

- [x] download necessary packages. Check if dependencies are up to date. Stable version.
      → refreshed to current stable: `rmcp 3.5.0` (the docs' pinned `0.3` no longer
      compiles), `tokio 1.53`, `reqwest 0.13`, `clap 4.6.7` (+`env` feature);
      `walkdir 2.5` added for the vault scan.
- [x] read comments on rust files to find out what to put there.
      → the four root placeholders became `src/ollama_client.rs` (Ollama HTTP and
      tool calls), `src/mcp_server.rs` (`#[tool_router]` server handler),
      `src/host.rs` (orchestration loop) and `src/main.rs` (clap CLI); the root
      copies were removed. `src/vault.rs` holds the retrieval core.
- [x] ollama reachable on http://localhost:11434/v1. use model gemma4:e4b
      → `mcp_obsidian doctor` reports Ollama 0.33.1 reachable, 12 models, and
      `gemma4:e4b` available. Both `/v1/chat/completions` and `/api/chat` work.
- [x] implement serverhandler
      → `ObsidianServer` exposes `search_by_tag`, `search_by_term`,
      `get_linked_notes` and `get_note`; verified with a full
      `initialize` → `tools/list` → `tools/call` handshake over stdio.
- [x] provide feedback, in `logs/MCP_notes.md` how this implementation differs,
      specifically if either did anything better.
      → written, including a flag that `AGENTS.md` (Python/.venv/pip) does not
      match this Rust workspace.
- [x] test rust app to see if it can communicate with ollama.
      → a plain round trip answered `pong` in 7.8 s; a tool-using question made the
      model call `search_by_term` then `get_note` and answer with a cited note path.
      Evidence is in the daily log.

## Follow-ups (not started)

- [ ] Return `CallToolResult::error` for genuine tool failures (see `SUMMARY.md` → Suggestions).
- [ ] Persist the vault index between runs (invalidate on file mtime change).
- [ ] Expose notes as MCP `resources` in addition to tools.
- [ ] Heading-scoped retrieval so a note section can be requested instead of a whole note.
- [ ] Fix `AGENTS.md` so its Python/.venv rules do not apply to this Rust workspace.
