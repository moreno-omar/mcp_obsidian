## MCP tool using obsidian

- going with the idea that it's better to search for local information using tags and search over RAG.
- more possible with the Open Knowledge Format (OKF) that Google is encouraging. Which mostly uses markdown.
- markdown, notes, and better access to local info makes me think there should be an app to connect an LLM to obsidian.

### Goals
- have tool check notes using: title, tags, and follow links in notes
- to have as a vault for llm to make notes. To solve problems, document it, and see if new problems are similar.


### Non-goals
- create a full GUI interface for the app
- to allow RAG use


## References
- https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/


## Status

Phase 1 complete (2026-09-28): the CLI can talk to Ollama and the tool set is
served over MCP.

Phase 2 complete (2026-10-01): the retrieval and Ollama layers are split into
small modules (`src/vault/`, `src/ollama_client/`) behind a library target, with
32 tests pinning the behaviour.

See `SUMMARY.md` for how it works and why, `ARCHITECTURE.md` for how to read the
code and where to change it, `logs/MCP_notes.md` for the implementation
comparison, and `TASKS.md` for what is done and what is next.

## Usage

```bash
cargo build --release

# vault-only commands (no model needed)
./target/release/mcp_obsidian --vault ~/vault index
./target/release/mcp_obsidian --vault ~/vault search tag project --match-all
./target/release/mcp_obsidian --vault ~/vault search term "token budget"
./target/release/mcp_obsidian --vault ~/vault search links index --depth 2
./target/release/mcp_obsidian --vault ~/vault search note projects/alpha

# with Ollama (defaults: http://localhost:11434/v1, gemma4:e4b)
./target/release/mcp_obsidian --vault ~/vault doctor
./target/release/mcp_obsidian --vault ~/vault ask "what do my notes say about token budgets?" --trace
./target/release/mcp_obsidian --vault ~/vault chat

# as an MCP server for a desktop host
./target/release/mcp_obsidian --vault ~/vault serve
```

A ready-to-edit host configuration is in `claude_desktop_config.example.json`.
Environment variables `OBSIDIAN_VAULT`, `OLLAMA_URL`, `OLLAMA_MODEL` and
`RUST_LOG` override the defaults; `--api native` switches from
`/v1/chat/completions` to `/api/chat`.

A sample vault lives in `samples/vault/` for trying the commands above.

## Tests

```bash
cargo test                            # vault + ollama client; no model needed
cargo test -- --ignored live_client   # needs a running Ollama with the model pulled
```

