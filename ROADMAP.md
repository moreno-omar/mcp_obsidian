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