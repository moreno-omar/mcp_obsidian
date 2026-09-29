# MCP_notes.md — implementation comparison & feedback

Date: 2026-09-28
Compares: the official MCP documentation example (Rust quickstart, `rmcp = "0.3"`)
against this implementation (`mcp_obsidian`, `rmcp = "3.5.0"`).

---

## 1. What was compared

| | Reference (MCP docs "Build an MCP server") | This project |
| :- | :- | :- |
| SDK | `rmcp = "0.3"` (pinned in `Cargo.toml` at repo start) | `rmcp = "3.5.0"` (latest stable on crates.io) |
| Example | `weather` server: `get_alerts`, `get_forecast` over HTTP to api.weather.gov | `mcp_obsidian`: `search_by_tag`, `search_by_term`, `get_linked_notes`, `get_note` over an in-memory vault index |
| Host | Claude for Desktop (no LLM loop in the sample) | Claude Desktop *or* the built-in Ollama loop (`ask` / `chat`) |
| Language tabs | Python, TS, Java, Kotlin, C#, Ruby, **Rust**, Go | Rust only |

Both were read as primary sources; the `rmcp` API facts below were verified against
the vendored crate source in `~/.cargo/registry/src/.../rmcp-3.5.0`, not from prose.

---

## 2. Differences that matter

### 2.1 SDK version drift (the docs snippet does not compile as-is against 3.5)

The documented Rust pattern is:

```rust
#[tool_handler]
impl ServerHandler for Weather {
    fn get_info(&self) -> ServerInfo {          // ← removed in 3.x
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
```

In `rmcp` 3.5.0:

* `ServerInfo` is gone; `get_info()` returns `ServerConfig`, which is a type alias
  for `InitializeResult` (`src/model.rs:1179`).
* `#[tool_handler]` generates `call_tool`, `list_tools`, `get_tool` **and**
  `get_info` (unless you define it yourself), and accepts
  `name` / `version` / `instructions` as string literals
  (`rmcp-macros-3.5.0/src/tool_handler.rs`).
* A no-boilerplate `ServerHandler` impl is therefore just:

```rust
#[tool_handler(name = "mcp_obsidian", version = "0.1.0", instructions = "...")]
impl ServerHandler for ObsidianServer {}
```

* `rmcp::transport::io::stdio()` exists as a helper; `ServiceExt::serve` works
  directly on the handler thanks to `impl<H: ServerHandler> Service<RoleServer> for H`.

**Better in the docs:** the prose is far clearer for a first server, and the
per-language tabs stay consistent. The `ServerInfo` snippet is also *correct for
0.3*, which is what it pins — the drift is really a docs-versions problem, not an
authoring mistake.

**Better here:** the version was checked against crates.io first and the API was
read from the crate source, so the code compiles against a current release. Anyone
copy-pasting the docs snippet today gets four compile errors and no migration note.

### 2.2 Tool implementation: fetch-per-call vs. pre-built index

The docs do real network I/O inside each tool and return `String`. This project
splits the layer in two:

* `src/vault.rs` — pure, synchronous, in-memory queries over an index built once at
  startup. No I/O at query time.
* `src/mcp_server.rs` — thin `#[tool]` wrappers that deserialize a request struct
  and call those functions.

**Better in the docs:** one file, one mental model, trivially readable. For a
tutorial that is the right call.

**Better here:** the same functions are reused by the local Ollama host loop, so
an MCP host and the built-in loop can never diverge in behaviour. It also means
retrieval can be verified with no model in the loop at all
(`mcp_obsidian search tag project`), which is what makes "deterministic and
reproducible" (SPEC.md) checkable.

### 2.3 Schema reuse (this project's biggest win)

`rmcp` derives JSON Schema from the tool parameter structs via `schemars`, but the
sample only ever uses that for MCP. Here the identical structs also generate the
Ollama `tools` array:

```rust
ToolSpec::new("search_by_tag", "...", schema_of::<SearchByTagRequest>())
```

So the MCP `inputSchema` and the OpenAI/Ollama `function.parameters` are generated
from one definition. Adding a field to `SearchByTagRequest` updates both
interfaces at once; they cannot drift.

### 2.4 The host loop (absent from the docs by design)

The docs quickstart deliberately delegates orchestration to Claude for Desktop.
Phase 1 of this project required talking to Ollama directly, so a loop was needed.
Decision: **dispatch tools in-process** instead of connecting to ourselves as an
MCP client over stdio.

Reason: `ToolRouter::call()` requires a `ToolCallContext`, which requires a real
`RequestContext<RoleServer>` containing a live `Peer` — it is not constructible
outside a running service. Going through stdio would mean spawning a second
process and owning a JSON-RPC pipe just to call our own functions.

**Better in the docs:** using a real MCP host as the client is dramatically less
code, and it gives you the user-approval flow, streaming UI and tool-call
rendering for free. If a GUI host is acceptable, that is the better product choice.

**Better here:** headless, no child process to supervise, no pipe to keep alive,
and tool failures are fed back to the model as ordinary text so it can correct
itself instead of the session ending.

### 2.5 Ollama wire formats — a difference the docs never have to face

api.weather.gov has one shape. Ollama has two, and they disagree in the details:

| | `/v1/chat/completions` (OpenAI) | `/api/chat` (native) |
| :- | :- | :- |
| `tool_calls[].function.arguments` | JSON **string** | JSON **object** |
| call correlation | `id` required, echoed in the tool result | usually absent |
| final text | `choices[0].message.content` | `message.content` |

`src/ollama_client.rs` normalises both into one `AssistantTurn`/`ToolCall` type and
synthesises `call_N` ids when the server omits them. Both paths were tested against
`gemma4:e4b` and both produced grounded answers. Keeping that translation in one
module means no caller above it knows which dialect is in use.

### 2.6 Stdio hygiene

The docs are explicit and correct: never write to stdout in a stdio server.
This project enforces it structurally — `tracing_subscriber` is initialised with
`.with_writer(std::io::stderr)` in `init_tracing`, so every log line, at every
verbosity, goes to stderr in every subcommand. Stdout is only ever used by CLI
output or the JSON-RPC stream. Verified: a full `initialize` → `tools/list` →
`tools/call` handshake produces 4 clean JSON lines on stdout and nothing else.

### 2.7 Tool error semantics

`rmcp` 3.5 documents the distinction sharply (`src/handler/server.rs:532`):
`Ok(CallToolResult::error(..))` reaches the user, `Err(ErrorData)` is an opaque
protocol error the caller cannot read.

This project returns tool failures as ordinary text (`ERROR: no note matches 'x'`).
**Better here** for an LLM consumer: the model sees the failure and can fix its own
call. **Better in the docs' guidance:** a real error result lets an MCP UI render a
red failure; here a client sees a successful call whose text says ERROR. A future
revision should return `Ok(CallToolResult::error(..))` for genuine failures while
keeping plain text for "no results matched".

### 2.8 Python (AGENTS.md) vs. Rust (this repo)

`AGENTS.md` prescribes Python 3.10+, a `.venv` and `pip`, but the repository is a
Cargo project (`edition = "2024"`) and `SPEC.md` names the deliverable as an MCP
stdio server with no Python requirement. The Python SDK tutorial is genuinely
faster to start with (a decorator and a type hint per tool), but it needs `uv` and
an interpreter at launch time.

**Better here for this project:** one static binary per release that drops straight
into an MCP client config with no runtime dependency, plus compile-time checking of
the tool argument structs. Note this as a documentation bug in `AGENTS.md`: its
Python/venv rules do not apply to a Rust workspace, and following them would
mislead a future contributor.

---

## 3. Verdict

* **The docs' approach is better** for onboarding, for GUI-hosted servers, and for
  the "one file, five minutes" demo. Its Claude-for-Desktop configuration section
  has no equivalent here yet.
* **This implementation is better** where it matters for a real tool: current SDK,
  verified API facts, one schema source for two model interfaces, deterministic
  retrieval that is testable without a model, a headless LLM loop, and a
  normalised bridge across Ollama's two incompatible tool-call dialects.
* **Worth adopting from the docs:** ship a `claude_desktop_config.json` example
  (done, see `SUMMARY.md`), and follow their `CallToolResult::error` guidance for
  real failures (see 2.7).
