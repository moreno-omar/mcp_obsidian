# Meta
- file to put useful sites and documentation as reference


### MCP - build server
- https://modelcontextprotocol.io/docs/2026-07-28/develop/build-server
  → the Rust tab is the reference server this project started from. Note it pins
    `rmcp = "0.3"`, which no longer compiles against 3.5 (`ServerInfo` is gone).
- https://modelcontextprotocol.io/docs/2026-07-28/learn/server-concepts
- https://github.com/modelcontextprotocol/quickstart-resources/tree/main/weather-server-rust
- https://modelcontextprotocol.io/docs/2026-07-28/tools/debugging

### Rust SDK (rmcp)
- https://docs.rs/rmcp/3.5.0/rmcp/ — `ServerHandler`, `ToolRouter`, `ServerConfig`,
  `transport::io::stdio`, `ServiceExt::serve`
- https://crates.io/crates/rmcp — version check (`rmcp` 0.3 → 3.5.0 between docs and now)
- https://docs.rs/schemars/1.2.2/schemars/ — `JsonSchema` derive + `schema_for!`,
  used for both MCP `inputSchema` and the Ollama `tools` array
- https://docs.rs/walkdir/2.5.0/walkdir/ — ordered, symlink-safe vault scan

### Ollama API
- https://github.com/ollama/ollama/blob/main/docs/api.md — `/api/chat`, `/api/tags`,
  `/api/version`, tool calling
- https://github.com/ollama/ollama/blob/main/docs/openai.md — OpenAI-compatible
  `/v1/chat/completions` (differences from native: arguments as string, call ids)
- https://ollama.com/library/gemma4 — model used in Phase 1

### Obsidian / frontmatter
- https://help.obsidian.md/links — wikilink and alias resolution rules
- https://help.obsidian.md/tags — inline `#tag` and nested `tag/subtag` semantics
- https://help.obsidian.md/properties — frontmatter property types
- https://github.com/wooorm/gray-matter (JS) / https://crates.io/crates/gray_matter —
  upgrade path if the hand-rolled frontmatter parser is not enough

### Open Knowledge Format
- https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/
