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