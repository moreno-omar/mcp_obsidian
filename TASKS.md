# TASKS.md - 2: Improve response

- [x] diagnose why the app fails to use the tool when the command `ask` is used.
  - Root cause: model behaviour, not the request. `gemma4:e4b` decides
    case-by-case whether a question is "vault-worthy"; measured on the same
    question, the old prompt called a tool 1/3 times and otherwise answered from
    its own knowledge (once refusing: "my capabilities are limited to searching
    … the vault"). `tool_choice: "required"` (ignored by Ollama's OpenAI
    endpoint), schema cleanup and `temperature: 0` were all measured and none of
    them force the call.
  - Fix: imperative protocol in `SERVER_INSTRUCTIONS` and all four tool
    descriptions, plus a deterministic pre-search (`Host::prefetch_notes`) that
    injects the top vault hits into the system prompt before the model runs.
  - Verified: 3/5 failures before → 0/5 after (`rounds: 2 | tool calls: >= 1`),
    8/8 runs called at least one tool, and 3/3 after the final prefetch tweak
    (prompt `get_note` when an excerpt is `…[truncated]`, `PREFETCH_CHARS` 800).
- [x] for `ask`, improve the output by using richrs with tokyo night theme.
  - New `src/ui/` (`theme.rs` palette, `markdown.rs` renderer, `mod.rs` views).
    Markdown answers render with headings/bullets/quotes/fenced code/inline
    code/links; `--trace` output moved to stderr with the same theme.
- [x] if possible, also improve output of chat using richrs.
  - Welcome panel (model, endpoint, tools, exit hint), teal `you>` prompt, blue
    `llm>` label, same markdown rendering, errors rendered in red.
- [x] if possible, also improve output of when used as MCP server.
  - Startup panel on **stderr** only (stdout stays pure JSON-RPC), stronger
    `instructions` field and tool descriptions, `serve -vv` logs every tool call,
    `doctor`/`index`/`search` restyled, closed stdout (`| head`) exits 0.

## Follow-up (2026-10-09, same day)

- [x] wrap long lines in `ui/markdown.rs` so the code-block gutter survives
      terminal wrapping (2026-10-09, later the same day: layout moved to
      `ui/wrap.rs`; prose word-wraps with aligned continuation lines, code
      hard-wraps at the last space; width `0` = no wrap, which is what piped
      output uses).
- [x] compress the demo GIFs before committing (`gifsicle -O1`: 46.8 MB + 36.7 MB
      → 144 KB + 406 KB, bit-identical frames and delays; see the 2026-10-09 log).
- [ ] render markdown tables (`richrs::Table`) — the model already emits them.
- [ ] consider `think: false` / `reasoning_effort` once re-measured (see
      SUMMARY.md suggestion 13).
- [ ] split `main.rs` (still the largest file; clap types + commands).
