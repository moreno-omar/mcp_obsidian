# TASKS.md - 2: Refactor

- [x] Create tests for vault that test the vault only commands. index. search based tag, term, links, note.
      → `tests/vault_queries.rs` (22 tests)
- [x] create tests that works on different address types
      → `note_lookup_accepts_every_address_type` covers path, `.md`, stem, title,
        alias, backslashes, `#heading`, `|alias`, fuzzy and error cases.
- [x] refactor vault.rs, the `impl Vault`, where the functions are split to best grouping across modules.
      → `src/vault/{index,query,render}.rs` hold one `impl Vault` each.
- [x] put helper functions in their own file.
      → `src/vault/text.rs` (pure string helpers), `src/vault/walk.rs` (scan
        rules), `src/vault/parse.rs` (markdown → `Note`).
- [x] create tests for ollama_client.rs to test that it can create the client, connect to it, and receive a response.
      → `tests/ollama_client.rs` (10 tests + 1 `--ignored` live test).
- [x] refactor ollama_client.rs, `impl OllamaClient`, where the functions are split to best grouping across modules.
      → `src/ollama_client/{client,types,wire,parse}.rs`.

## Follow-ups (not in scope for phase 2)

- [ ] Split `src/main.rs` (414 lines): `cli.rs` for the clap types, `commands.rs`
      for the subcommand bodies. It is the largest file left.
- [ ] Return `CallToolResult::error` for real tool failures (see SUMMARY.md §5.1).
