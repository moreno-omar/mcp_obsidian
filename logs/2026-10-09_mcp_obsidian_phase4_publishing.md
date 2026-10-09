# 2026-10-09 — mcp_obsidian Phase 4 (marketplace packaging)

## 15:55 — Phase 4: make the binary publishable, and find out what publishing actually requires

### Objective

Implement every unchecked item in `TASKS.md` (Phase 4 of `ROADMAP.md`, *"to have
app available in OpenAI ChatGPT App Directory … publish to Official MCP
Registry"*):

1. confirm the app is a compliant MCP server;
2. create and fill out `server.json`;
3. publish to crates.io;
4. publish to Smithery;
5. publish to MCP.so.

And, per the request, report which accounts have to be registered. The stated
goal is the OpenAI ChatGPT App Directory.

### Solution

**Compliance was proved, not assumed.** The release binary was driven through a
raw stdio session with hand-written JSON-RPC frames — `initialize` (protocol
`2025-06-18`), `notifications/initialized`, `tools/list`, `tools/call`,
`resources/list`, and a deliberate call to an unknown tool. The server negotiates
the protocol, advertises `capabilities.tools`, returns a correct `serverInfo` and
`instructions` string, returns four tools with JSON Schema, returns
`isError: false` with a `content` array, and answers `-32602 "tool not found"` for
the unknown tool. Nothing needed changing.

**The crate could not be published at all**, which was the real blocker:
`Cargo.toml` had no `description` and no `license`, and there was no `LICENSE`
file. Added `description`, `license = "MIT OR Apache-2.0"` (the user's choice),
`repository`, `homepage`, `readme`, `keywords`, `categories`,
`rust-version = "1.88"` — the floor `rmcp`'s own dependency tree imposes, measured
from `cargo metadata` rather than guessed — and an `exclude` list so the published
crate carries source and fixtures but not `logs/` or the dev notes. Wrote
`LICENSE-MIT` and fetched `LICENSE-APACHE` verbatim from apache.org (checksum and
text verified) with the copyright line filled in.

**`server.json` was validated against the live registry.** `mcp-publisher` v1.8.1
was downloaded from the official release page, its SHA-256 checked against
`registry_1.8.1_checksums.txt`, installed to `~/.local/bin`, and run:
`mcp-publisher validate server.json` → `✅ server.json is valid`. Two rules that
are not guessable were confirmed by reading the validator's own Go source: the
registry name must be reverse-DNS inside a namespace you prove you own
(`io.github.moreno-omar/mcp-obsidian` — the GitHub account is a *user*, so GitHub
OAuth suffices), and for `registryType: "cargo"` the package→server binding is
proved by finding the literal text
`mcp-name: io.github.moreno-omar/mcp-obsidian` in the crate's **rendered** README,
followed by a boundary character.

**The README token is the part that silently fails.** crates.io strips HTML
comments before rendering, so unlike PyPI and NuGet the token cannot be hidden in
`<!-- … -->`; it has to be visible markdown. It was added as its own line in the
README's *Marketplaces* section, with a note explaining why it must stay there.

**Smithery ships local servers as MCPB bundles.** Smithery's current flow takes
either a remote Streamable-HTTP URL or a local MCPB bundle; only the bundle can
work for a stdio binary. Wrote `mcpb/manifest.json` (`manifest_version: "0.3"`,
`server.type: "binary"`, `entry_point: "server/mcp_obsidian"`, an `mcp_config`
running `${__dirname}/server/mcp_obsidian --vault ${user_config.vault} serve`, and
a `user_config.vault` **directory** prompt so the host asks for the vault path
instead of relying on an environment variable), and `scripts/build_mcpb.sh`, which
builds the release binary, stages `manifest.json` + `server/mcp_obsidian`, and zips
`dist/mcp_obsidian-<version>-<platform>.mcpb`. The script refuses to build when
`Cargo.toml` and the manifest disagree on the version.

**The ChatGPT directory is not reachable from stdio, at any price.** OpenAI's
plugin review requires "your MCP server is hosted on a publicly accessible domain"
and rejects local or testing endpoints, then adds OAuth, domain verification and
organisation verification. `SPEC.md` scopes this project to stdio and local-first,
so this is a product decision (host a Streamable-HTTP transport) rather than a
packaging step. Reported as a blocker rather than worked around.

**Accounts required** (documented in the new `PUBLISHING.md`): crates.io
(GitHub sign-in + an API token), the Official MCP Registry (existing GitHub
account, device-code flow), Smithery (GitHub sign-in), MCP.so (Google/email
sign-in, $39 for the instant listing). Glama auto-indexes GitHub. PulseMCP is
paused and re-indexes the official registry itself.

### Actions Performed

| # | Command / edit | Effect |
| :- | :- | :- |
| 1 | `cargo build --release` | confirmed the shipped binary builds clean before probing it |
| 2 | stdio JSON-RPC probe (`initialize`, `tools/list`, `tools/call`, bad tool) | proved MCP compliance; found no defect |
| 3 | `curl` GitHub + crates.io + registry APIs | owner is a **user** (`moreno-omar`, Omar Moreno Duran); `mcp_obsidian`, `mcp-obsidian`, `mcpobsidian` free; `obsidian-mcp` taken |
| 4 | `cargo metadata` | measured MSRV = 1.88 from the dependency tree |
| 5 | created `LICENSE-MIT`; `curl` → `LICENSE-APACHE` | dual licence; Apache text taken verbatim from apache.org |
| 6 | edited `Cargo.toml` | description, licence, repo, keywords, categories, `rust-version`, `exclude` |
| 7 | created `server.json` | registry entry, schema `2025-12-11`, `registryType: "cargo"`, stdio |
| 8 | downloaded + `sha256sum`-verified `mcp-publisher`, `install -m 0755` | publisher at `~/.local/bin/mcp-publisher` |
| 9 | `mcp-publisher validate server.json` | `✅ server.json is valid` against the live registry |
| 10 | fetched + read `internal/validators/registries/{cargo,mcpname}.go` | confirmed the exact token format and trailing-boundary rule |
| 11 | edited `README.md` | visible `mcp-name:` token, Install/Marketplaces/License sections, Phase 4 status |
| 12 | `cargo update -p yoke-derive` | cleared a yanked `0.8.3` → `0.8.4` (1 package, `Cargo.lock` only) |
| 13 | `cargo publish --dry-run --allow-dirty` | packaged 50 files / 1.6 MiB (1.3 MiB compressed) and compiled from the tarball |
| 14 | created `mcpb/manifest.json` | MCPB manifest for Smithery / Claude Desktop |
| 15 | created `scripts/build_mcpb.sh`, `chmod +x`, ran it | produced `dist/mcp_obsidian-0.1.0-linux.mcpb` (5.0 MiB) |
| 16 | `npx ajv-cli` against `mcpb-manifest-latest.schema.json` | manifest valid |
| 17 | extracted the bundle and re-ran the stdio probe | the packaged binary serves MCP correctly |
| 18 | `.gitignore` += `/dist` | build output not committed |
| 19 | edited `TASKS.md`, `ROADMAP.md`, `SUMMARY.md`, `ARCHITECTURE.md` | state, status, §0.2 write-up, packaging-files map |
| 20 | created `PUBLISHING.md` | the release runbook: accounts, commands, publish order |

### Verification Output

MCP compliance, from the raw stdio probe:

```
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},
 "serverInfo":{"name":"mcp_obsidian","version":"0.1.0"},"instructions":"Read-only, deterministic …"}}
{"jsonrpc":"2.0","id":2,"result":{"tools":[ … "get_linked_notes","get_note","search_by_tag","search_by_term" … ]}}
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"# Vault Index\n…"}],"isError":false}}
{"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"tool not found"}}
```

Registry metadata:

```
$ mcp-publisher validate server.json
Validating against https://registry.modelcontextprotocol.io...
✅ server.json is valid
```

Crate packaging:

```
   Packaged 50 files, 1.6MiB (1.3MiB compressed)
   Verifying mcp_obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
   Compiling mcp_obsidian v0.1.0 (…/target/package/mcp_obsidian-0.1.0)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.72s
   Uploading mcp_obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
warning: aborting upload due to dry run
```

Bundle, and the packaged binary serving MCP from the extracted bundle layout:

```
$ ./scripts/build_mcpb.sh
wrote /home/redram/rust/mcp_obsidian/dist/mcp_obsidian-0.1.0-linux.mcpb (5.0M)

$ npx ajv … validate -s mcpb-manifest-latest.schema.json -d mcpb/manifest.json
/home/redram/rust/mcp_obsidian/mcpb/manifest.json valid

$ (extract bundle, then drive $PWD/server/mcp_obsidian over stdio)
initialize OK  -> {'name': 'mcp_obsidian', 'version': '0.1.0'}
tools/call OK  -> isError=False, 1197 chars
--- first line ---
2 note(s) matched the term query 'vault, index':
```

Name availability (`https://crates.io/api/v1/crates/<name>`):

```
mcp_obsidian   -> HTTP 404  (free)
mcp-obsidian   -> HTTP 404  (free)
obsidian-mcp   -> HTTP 200  (taken)
```

### References Used

1. Official MCP Registry — `server.json` schema `2025-12-11` and package types
   (the `cargo`/`stdio` shape, and the README-token rule):
   <https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json>
   and <https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx>
2. MCP Bundle (MCPB) manifest specification — `server.type: "binary"`,
   `entry_point`, `${__dirname}` / `${user_config.*}` substitution:
   <https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md>
3. OpenAI plugin (ChatGPT app) review requirements — remote HTTPS MCP server,
   rejection of local endpoints, OAuth and domain/organisation verification:
   <https://developers.openai.com/plugins/deploy/app-review>

### Optional steps

1. **Reserve the crate name now.** crates.io is first-come-first-serve; a 0.1.0
   publish today is cheaper than discovering a squatter later. `cargo login` +
   `cargo publish` are the only two commands needed.
2. **Automate both publishes in CI.** `cargo publish` on a tag plus
   `mcp-publisher login github-oidc` + `publish` would make the "crate first,
   registry second" ordering structural; it needs a `CARGO_REGISTRY_TOKEN` secret
   and `id-token: write`.
3. **Add an icon and screenshots to the MCPB manifest.** The schema accepts both
   and the repo already has the demo GIFs and PNGs; marketplace listings convert
   far better with them.
4. **Cross-build the bundle.** `scripts/build_mcpb.sh` only bundles the host
   platform; macOS and Windows need `cargo-zigbuild`/`cross` or a CI matrix, and
   that is also the only path to code-signing them.
5. **Add the `.mcp/server.json` convention** if VS Code / Copilot should pick the
   server up from an installed package, or leave the single root file and pass its
   path to `mcp-publisher publish`.
6. **Fix the `Direcotry` typo** in the `TASKS.md` and `ROADMAP.md` headings (left
   alone here as unrelated to the change).

### Possible problems

1. **The OpenAI exit criterion is unmet and cannot be met from stdio.** Phase 4's
   exit criteria explicitly say "Have app show up in OpenAI directory". That needs
   a hosted remote Streamable-HTTP server plus domain and organisation
   verification, and `SPEC.md` currently excludes hosting. Either revise the exit
   criteria or accept the hosting work as a new phase.
2. **Publish order is load-bearing.** The registry verifies the crate's rendered
   README, so `cargo publish` must run **before** `mcp-publisher publish`, and the
   `mcp-name:` token must be in the published version. Reversing the order fails
   with an error that reads like a namespace/permissions problem.
3. **Version numbers live in three files** — `Cargo.toml`, `server.json`,
   `mcpb/manifest.json`. `build_mcpb.sh` checks the crate against the manifest but
   nothing checks `server.json`, and a registry publish with a stale version fails
   because the crate version must already exist on crates.io.
4. **The registry name is tied to the GitHub account.** `io.github.moreno-omar/*`
   is claimable only while that account owns it; renaming the account orphans the
   listing.
5. **MCP.so may not have a free path any more.** Its own metadata advertises a free
   review, but the live form only exposed a $39 instant listing. Budget for the
   fee.
6. **An MCPB bundle is unsigned native code.** On macOS and Windows an unsigned
   binary triggers Gatekeeper/SmartScreen; clearing that needs paid Apple and
   Windows signing certificates.
7. **`rmcp` is pre-1.0.** The MCP surface is compliant today, but a minor bump can
   invalidate the `#[tool]`/`#[tool_handler]` shape that `mcp_server.rs` depends on.
8. **Nothing was actually published.** All four listings are still pending because
   they need the author's accounts and credentials; only packaging and validation
   were done here. The tasks remain open in `TASKS.md` under "Blocked on accounts".

## 16:01 — Phase 4b: add a Streamable-HTTP transport so the directory goal is reachable

### Objective

Remove the structural blocker found above: the OpenAI/plugin directory requires a
**remote** MCP server over HTTPS, and `serve` only spoke stdio, so the server could
not be reached without spawning the process. Add Streamable HTTP as a second
transport over the same retrieval core, without changing the stdio default.

### Solution

`Serve` became `Serve { http: Option<String>, allow_host: Vec<String> }`.
`--http` alone binds `127.0.0.1:8000` (`default_missing_value`), `--http
0.0.0.0:8000` exposes it, and stdio stays the default so stdout remains the
JSON-RPC channel. `run_serve_http` builds **one** `StreamableHttpService` whose
factory hands every session an `ObsidianServer` over the *same* `Arc<Vault>` —
the vault is still indexed exactly once, not per connection — nests it at `/mcp`
in an `axum` router, and serves it with `axum::serve`.

`rmcp` ships the service but no listener, so `axum 0.8` (`http1`, `tokio`, no
default features) was added as a dependency; that is the only new crate, and it
is what rmcp's own server tests use.

**The `Host` guard is the part that silently breaks a deployment.**
`StreamableHttpServerConfig::default()` accepts only loopback `Host` headers —
rmcp's DNS-rebinding defence — so a public hostname gets `403` on every request.
Rather than leave that as a "change the source" note, `--allow-host HOST`
(repeatable, `requires = "http"`) was added: `http_config` starts from the
loopback default and extends it, so `--allow-host vault.example.com` still answers
on `127.0.0.1` and an empty list keeps the server unreachable under any other
name. `ui::print_serve_banner` gained a `transport` parameter so the startup panel
names whichever transport is live, and the duplicated tool-name collection was
pulled into `fn tool_names()`.

### Actions Performed

| # | Command / edit | Effect |
| :- | :- | :- |
| 1 | read `rmcp-3.5.0/src/transport/streamable_http_server/{.rs,tower.rs}`, `session/local.rs`, `tests/test_server_discover_http.rs` | established the real API: `StreamableHttpService::new(factory, session_manager, config)` + `handle`, `LocalSessionManager`, loopback-only default `allowed_hosts`, and that axum `nest_service` works |
| 2 | `Cargo.toml`: added rmcp feature `transport-streamable-http-server`, added `axum 0.8` | the transport and a listener to serve it |
| 3 | `src/ui/mod.rs`: `print_serve_banner(…, transport: &str)` | the panel can name either transport |
| 4 | `src/main.rs`: `Serve { http, allow_host }`, dispatch, `tool_names()` | CLI surface |
| 5 | `src/main.rs`: `run_serve_http` + `http_config` | the server itself, and the `Host` allow-list policy |
| 6 | `cargo build --release` | compiles |
| 7 | started `serve --http 127.0.0.1:8765`, drove it with `curl` | verified the full MCP flow over HTTP |
| 8 | `curl -H 'Host: evil.example.com'` | confirmed the 403 DNS-rebinding guard |
| 9 | restarted with `--allow-host vault.example.com` | confirmed that host is admitted while others stay 403 |
| 10 | re-ran the stdio probe | regression check: unchanged |
| 11 | edited `README.md`, `SPEC.md`, `ARCHITECTURE.md`, `SUMMARY.md`, `PUBLISHING.md`, `TASKS.md`, `ROADMAP.md` | docs, including refreshing every `main.rs` line reference that shifted |

### Verification Output

Full MCP flow over Streamable HTTP (session id captured from the response header):

```
session id: ade56aa8-df76-4c90-9932-53711ebbfff9
notifications/initialized -> HTTP 202
tools/list -> ['get_linked_notes', 'get_note', 'search_by_tag', 'search_by_term']
tools/call -> isError=False, 1181 chars
first line  -> 2 note(s) matched the term query 'link, graph':
```

The `Host` guard, and the `--allow-host` escape hatch:

```
Host 127.0.0.1           -> HTTP 200
Host vault.example.com   -> HTTP 200
Host evil.example.com    -> HTTP 403  (expected)
POST /nope               -> HTTP 404

$ ./target/release/mcp_obsidian serve --allow-host x.example.com
error: the following required arguments were not provided:
  --http [<ADDR>]
```

Startup panel and the rejection log:

```
╭─ mcp_obsidian · serve ───────────────────────────────────────────────────────╮
│ transport streamable HTTP · http://127.0.0.1:8765/mcp                        │
│ vault     /home/redram/rust/mcp_obsidian/samples/vault                       │
│ indexed   4 notes, 11 tags, 0 unresolved links                               │
│ tools     search_by_tag, search_by_term, get_linked_notes, get_note          │
╰──────────────────────────────────────────────────────────────────────────────╯
WARN rejected request with disallowed Host header (possible DNS rebinding attempt) host=NormalizedAuthority { host: "evil.example.com", port: None }
```

stdio regression check:

```
initialize -> {'name': 'mcp_obsidian', 'version': '0.1.0'} 2025-06-18
tools/list -> 4 tools
```

Re-running the packaging and bundle steps after the transport landed gives **51
files / 1.4 MiB compressed** and a **5.8 MiB** bundle, versus 50 / 1.3 MiB and
5.0 MiB before it; the difference is `axum` plus the `hyper`/`tower` it already
pulled in transitively. (A stray `keys.md` that appeared during the session is
now excluded, which is why the count is 51 and not 52.) `mcp-publisher validate
server.json` still passes and `serve` over stdio is behaviourally unchanged.

### References Used

1. MCP Streamable HTTP transport specification (single `/mcp` endpoint, `POST`,
   `Mcp-Session-Id`, SSE or JSON responses):
   <https://modelcontextprotocol.io/specification/2025-06-18/basic/transports>
2. `rmcp` server-side HTTP API — `StreamableHttpService`, `StreamableHttpServerConfig`
   (`allowed_hosts` DNS-rebinding default), `LocalSessionManager`:
   <https://docs.rs/rmcp/3.5.0/rmcp/transport/streamable_http_server/index.html>
3. `axum` 0.8 routing and `serve` (`nest_service`, `axum::serve`):
   <https://docs.rs/axum/0.8.9/axum/fn.serve.html>

### Optional steps

1. **Add Origin validation.** `allowed_origins` is empty by default, which
   disables `Origin` checks for backward compatibility; a browser-facing
   deployment should set it with `with_allowed_origins`.
2. **Add OAuth.** `rmcp` has an `auth` feature. Until then the transport is
   unauthenticated and must stay behind a proxy.
3. **Share session state.** `LocalSessionManager` is in-memory, so a
   multi-instance deployment needs a shared session store (rmcp supports one).
4. **Add a `/healthz` route** to the router for load balancers and uptime checks.
5. **Serve TLS directly** (`axum-server` + rustls) if a reverse proxy is unwanted;
   today TLS is the proxy's job.
6. **Bound request bodies / add rate limiting** at the proxy, since the transport
   performs no request-size limiting of its own.
7. **Add an integration test** that boots `run_serve_http` on port 0 and drives
   the handshake, so the transport is covered by CI rather than by hand.

### Possible problems

1. **The HTTP transport has no authentication.** Anyone who can reach the port can
   read the vault. The loopback default plus the `Host` guard limit exposure but do
   not authenticate; this is the highest-risk item added today.
2. **`--allow-host` is a footgun if used carelessly.** Adding a wildcard-ish or
   over-broad name weakens the DNS-rebinding defence that rmcp provides by default.
3. **TLS is assumed to be terminated elsewhere.** `serve --http` speaks plain
   HTTP/1.1; exposing it directly to the internet would send vault content in
   cleartext.
4. **`axum` is a new dependency and a new supply-chain surface.** It was required
   because rmcp provides the service but no listener, and it pulls in
   `hyper`/`tower` already present transitively — but it is a dependency that did
   not exist yesterday.
5. **Sessions are per-process and in-memory.** A restart drops every session; a
   horizontally scaled deployment would break clients without a shared store.
6. **The OpenAI directory still cannot be reached today.** The transport was the
   code-shaped blocker; hosting, OAuth, domain verification and organisation
   verification all remain, and none of them are code in this repository.
7. **Doc line references drift.** Every `main.rs` reference in `ARCHITECTURE.md`
   had to be recomputed after the edit; nothing verifies them automatically, so
   they will rot again on the next change to `main.rs`.
8. **The packaged crate is guarded by a denylist, not an allowlist.** `Cargo.toml`
   uses `exclude`, so *any* stray file dropped in the repository root ships to
   crates.io unless it is listed. This was caught live: a personal `keys.md`
   (2 lines, no credential patterns) appeared during this session and was in the
   `cargo package --list` output. It is now excluded, but an `include` allowlist
   would fail closed instead. Because crates.io publishes are permanent, this is
   the one mistake that cannot be undone — check `cargo package --list` before
   every `cargo publish`.

## 16:22 — Phase 4c: rename to `mcp-obsidian`, use the real crates.io token

### Objective

Take the Phase 4 packaging the last mile: publish to crates.io with the supplied
API token (`keys.md`, to be erased afterwards), under the name `mcp-obsidian`.
Defer the Official MCP Registry, OpenAI and MCP.so. Also answer whether the
ChatGPT directory needs a server URL and whether a VPS can host it.

### Solution

**Named the crate `mcp-obsidian`.** The registry identity, `server.json` and the
intended listing were already hyphenated; only the crate was `mcp_obsidian`.
crates.io normalises `-` and `_` to the *same* name, so whichever spelling is
published first reserves both — publishing the underscore form would have frozen
a name that disagrees with everything user-facing.

**The rename is deliberately asymmetric.** `Cargo.toml` sets
`name = "mcp-obsidian"` and names the executable explicitly:

```toml
[[bin]]
name = "mcp-obsidian"
path = "src/main.rs"
```

That `[[bin]]` block is load-bearing: the MCPB manifest, the host-config example
and the README all invoke the executable by name, so pinning it stops a future
package rename from silently changing the contract the bundle depends on. The
**library crate path is still `mcp_obsidian`** — Cargo normalises `-` to `_` for
the crate identifier — so `use mcp_obsidian::…`, both test files and
`src/mcp_server.rs` needed no change. Only user-visible names moved: the package,
the binary, clap's `name`, `serverInfo.name`, the two `ui` panel titles, the
`doctor` hint line, `build_mcpb.sh`'s `BINARY`, and `server.json`'s `identifier`.

The doc rewrite had to avoid three kinds of incidental string that legitimately
keep the underscore: the `demo_mcp_obsidian_*.gif` and `mcp_obsidian_chat.png`
asset names, the `moreno-omar/mcp_obsidian` git remote, and the
`logs/…_phase4_publishing.md` log filename. A protect-then-replace pass over the
six docs did the rest.

**Authenticated and erased the key competently.** The token was piped straight
into `cargo login` (`grep -oE … | cargo login`) so it never appeared in the
command line or the transcript; `~/.cargo/credentials.toml` was created `0600`.
The masked inspection earlier confirmed the file held one 35-character token and
no credential patterns. `keys.md` was deleted **before** the publish attempt, so
it could not be packaged even in principle, and it stays on `Cargo.toml`'s
`exclude` list as a second line of defence.

**The publish reached the upload and was refused — by email, not by token.**

```
error: failed to publish mcp-obsidian v0.1.0 to registry at https://crates.io
Caused by:
  the remote server responded with an error (status 400 Bad Request): A verified
  email address is required to publish crates to crates.io.
  Visit https://crates.io/settings/profile to set and verify your email address.
```

`403` would mean a bad token; `400` at the upload means the token is fine and the
account is gated. The fix is a web step and cannot be done from the CLI, so
everything else was finished to leave the publish as one command.

### Actions Performed

| # | Command / edit | Effect |
| :- | :- | :- |
| 1 | masked inspection of `keys.md` | 2 lines: a heading and one 35-char token; no other secret patterns |
| 2 | `grep -oE … \| cargo login` | authenticated; `~/.cargo/credentials.toml` written `0600`, token never printed |
| 3 | `Cargo.toml`: `name = "mcp-obsidian"`, added `[[bin]]`, refreshed `description` | package renamed, executable pinned |
| 4 | `src/main.rs`, `src/mcp_server.rs`, `src/ui/mod.rs` | clap `name`, `serverInfo.name`, panel titles, `doctor` hint → `mcp-obsidian` |
| 5 | `server.json`, `mcpb/manifest.json`, `claude_desktop_config.example.json`, `scripts/build_mcpb.sh` | identifier, entry point, command, `BINARY` |
| 6 | protect-then-replace over 6 docs | binary references renamed; asset filenames, remote and log filename preserved |
| 7 | `cargo build --release` | `target/release/mcp-obsidian` produced |
| 8 | `cargo clippy --all-targets` / `cargo test` | clean; 32 tests pass, unchanged |
| 9 | `./scripts/build_mcpb.sh` | `dist/mcp-obsidian-0.1.0-linux.mcpb`, `server/mcp-obsidian` inside |
| 10 | removed stale `dist/mcp_obsidian-*.mcpb` and `dist/mcpb/` | no misleading old artefacts |
| 11 | `cargo package --list` | `keys.md` absent (exclude working) |
| 12 | `rm keys.md` | token file erased before publishing |
| 13 | `cargo publish --allow-dirty` | packaged, compiled from tarball, uploaded → refused on unverified email |
| 14 | edited `PUBLISHING.md`, `TASKS.md`, `ROADMAP.md`, `SUMMARY.md` | email prerequisite, deferred scope, status |

### Verification Output

Rename, both identities:

```
$ ./target/release/mcp-obsidian --version
mcp-obsidian 0.1.0
$ ./target/release/mcp-obsidian --help | head -3
Usage: mcp-obsidian [OPTIONS] <COMMAND>

$ printf '…initialize…' | mcp-obsidian --vault samples/vault serve
{'name': 'mcp-obsidian', 'version': '0.1.0'}
```

Quality gates after the rename:

```
$ cargo clippy --all-targets
    Checking mcp-obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
    Finished `dev` profile … in 0.73s

$ cargo test
test result: ok. 10 passed; 0 failed; 1 ignored        (ollama_client)
test result: ok. 22 passed; 0 failed; 0 ignored        (vault_queries)
```

Packaging, and the secret check:

```
$ cargo package --list | grep -c keys.md
0
$ cargo publish --dry-run --allow-dirty
   Packaged 51 files, 1.7MiB (1.4MiB compressed)
   Verifying mcp-obsidian v0.1.0
    Finished `dev` profile … in 6.14s
   Uploading mcp-obsidian v0.1.0
warning: aborting upload due to dry run
```

Bundle:

```
$ ./scripts/build_mcpb.sh
wrote …/dist/mcp-obsidian-0.1.0-linux.mcpb (5.8M)
$ unzip -l dist/mcp-obsidian-0.1.0-linux.mcpb
     1791  manifest.json
        0  server/
 19235720  server/mcp-obsidian
```

The publish, and the one thing blocking it:

```
$ cargo publish --allow-dirty
   Packaged 51 files, 1.7MiB (1.4MiB compressed)
   Uploading mcp-obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
error: failed to publish mcp-obsidian v0.1.0 to registry at https://crates.io
Caused by:
  the remote server responded with an error (status 400 Bad Request): A verified
  email address is required to publish crates to crates.io.
  Visit https://crates.io/settings/profile to set and verify your email address.
```

### References Used

1. crates.io publishing requirements — including the verified-email gate and
   permanent versions: <https://doc.rust-lang.org/cargo/reference/publishing.html>
2. crates.io name uniqueness (hyphen/underscore are the same name) and the API
   token page: <https://doc.rust-lang.org/cargo/reference/registries.html> and
   <https://crates.io/settings/profile>
3. Cargo manifest `[[bin]]` target naming for an executable whose name differs
   from the crate identifier:
   <https://doc.rust-lang.org/cargo/reference/cargo-targets.html>

### Optional steps

1. **Verify the email, then publish.** One command remains:
   `cargo publish --allow-dirty`, then
   `cargo install mcp-obsidian && mcp-obsidian --version`.
2. **Add `keys.md` to `.gitignore`.** It is on the package `exclude` list, but
   ignoring it in git too removes the chance of it ever being committed.
3. **Switch `exclude` to an `include` allowlist.** A denylist fails open: one
   stray file in the repository root reaches crates.io permanently.
4. **Commit before publishing properly.** `--allow-dirty` was needed because the
   work is uncommitted; a tagged commit is the cleaner provenance, and it is what
   a CI publish would key on.
5. **Add the `--locked` install hint** to the README once the crate is live, so
   users get the dependency set this was tested against.
6. **Rebuild the bundle after the rename lands**, and keep the `.mcpb` out of git
   (`dist/` is already ignored) while attaching it to a GitHub release.

### Possible problems

1. **The publish is still not done.** It is one web step from done, but until the
   email is verified the crate name `mcp-obsidian` is **not** reserved —
   crates.io is first-come-first-serve, and losing it would force another rename
   through every doc, the bundle manifest and `server.json`.
2. **Deleting `keys.md` does not revoke the token.** It is still live in
   `~/.cargo/credentials.toml` and on crates.io; anyone with read access to that
   file can publish as the account owner. Revoke it at <https://crates.io/me> when
   it is no longer needed, and prefer a scoped `publish-new`/`publish-update`
   token over a full-access one.
3. **The token was supplied in a plain file in the repository root.** It happened
   to be caught because it appeared in `cargo package --list`, but a `.gitignore`
   and an allowlist are the structural fixes; a secrets manager or an
   environment variable is better than a markdown file.
4. **The executable name is now load-bearing in four places.** `Cargo.toml`
   (`[[bin]]`), `mcpb/manifest.json` (entry_point + command),
   `claude_desktop_config.example.json` and the README must move together;
   nothing enforces that, and a mismatch surfaces only when a host fails to launch
   the server.
5. **`cargo install mcp-obsidian` and an existing `mcp_obsidian` binary can
   coexist.** Anyone who installed or built the old name keeps a stale
   `~/.cargo/bin/mcp_obsidian` that shadows nothing but will confuse debugging.
6. **A hosted vault changes the threat model.** The OpenAI directory needs a
   public HTTPS endpoint and the transport has no auth yet, so the vault's
   contents would be readable by anyone who can reach the URL. A VPS can host it;
   hosting should wait for OAuth.
7. **The library name and the package name now differ** (`mcp_obsidian` vs
   `mcp-obsidian`), which is normal Rust but easy to trip over when grepping:
   `use mcp_obsidian::…` is correct and must not be "fixed".

## 16:28 — Phase 4d: published `mcp-obsidian` v0.1.0, and wrote the VPS/Smithery/OpenAI runbook

### Objective

The crates.io account's email was verified, so run the publish that §16:22 left at
the upload gate. Then write a markdown file covering what is needed to get the MCP
server answering on a **public URL on a VPS** and submit it to **Smithery** and the
**ChatGPT/OpenAI** directory, since both need a hosted URL rather than a local
process.

### Solution

**Published.** `cargo publish --allow-dirty` — the `--allow-dirty` is required
because AGENTS.md forbids committing automatically, so the working tree is
intentionally dirty:

```
   Packaged 51 files, 1.7MiB (1.4MiB compressed)
   Verifying mcp-obsidian v0.1.0
   Uploading mcp-obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
    Uploaded mcp-obsidian v0.1.0 to registry `crates-io`
   Published mcp-obsidian v0.1.0 at registry `crates-io`
```

**Verified the registry prerequisite instead of assuming it.** The MCP Registry's
cargo validator fetches the crate's *rendered* README from the static CDN and
searches for the `mcp-name:` token, so the only way to know the registry publish
will work is to reproduce that fetch. Doing it by hand confirmed the token is
present as visible text with a valid trailing boundary (`<`) and is **not** inside
an HTML comment — crates.io strips those, which is why the token has to be plain
prose in the README. The Official MCP Registry step is therefore now a single
`login github` away, with the namespace as the only outstanding piece.

**Wrote `DEPLOYMENT.md`** as the requested runbook. It is deliberately ordered
around the fact that the *code* gap is OAuth, not hosting:

- **What you need** table (VPS, domain, TLS, vault-on-the-server, OAuth).
- **Step 1 DNS/firewall** — the app port stays closed; only 80/443 open.
- **Step 2 install + vault sync** — notes on the VPS, since retrieval reads local
  disk; the server never writes, so a one-way sync or read-only mount is safe.
- **Step 3 systemd** — a `--system` no-login account and a hardened unit
  (`ProtectSystem=strict`, `ReadOnlyPaths=/srv/vault`, `NoNewPrivileges`, …), plus
  the point that `--allow-host` is **mandatory** for a public name because rmcp
  rejects non-loopback `Host` headers.
- **Step 4 TLS** — Caddy and nginx variants; both must disable proxy buffering and
  use long timeouts or the SSE stream hangs and 502s. This is the single most
  likely deployment failure.
- **Step 5 verification** — the exact two `curl` calls (initialize + session id,
  then `tools/list`) and a symptom→cause table.
- **Step 6 OAuth** — named honestly as the one real code gap, with the four
  required pieces (authorization-server metadata, protected-resource metadata,
  DCR/CIMD/client-secret registration, bearer validation) and the note that
  `rmcp` has an `auth` feature.
- **Step 7 Smithery** — both routes: the URL route this document sets up, and the
  MCPB bundle route that needs neither VPS nor OAuth and is already built.
- **Step 8 OpenAI** — a checkbox list (public HTTPS, Streamable HTTP, OAuth, the
  plain-text `.well-known/openai-apps-challenge` domain token, verified org,
  ToS/privacy — which matters more here because the app reads the user's notes)
  and the review package (5 positive + 3 negative cases, reviewer credentials with
  no MFA, a video, one version in review).
- **Order to do it in**, with the hosting trade-off called out as a decision
  rather than a task.

### Actions Performed

| # | Command / edit | Effect |
| :- | :- | :- |
| 1 | `cargo publish --allow-dirty` | **published `mcp-obsidian` v0.1.0 to crates.io** |
| 2 | `GET /api/v1/crates/mcp-obsidian` | confirmed name, version, `MIT OR Apache-2.0`, repo |
| 3 | `GET …/0.1.0/readme` → follow the CDN URL → search the HTML | ownership token present, visible, valid boundary; not an HTML comment |
| 4 | created `DEPLOYMENT.md` | the VPS → URL → Smithery → OpenAI runbook |
| 5 | edited `README.md` | crates.io badge link, install block, links to both runbooks, Phase 4 status |
| 6 | edited `TASKS.md` | crates.io marked published; token-revocation task added; OpenAI section rewritten as hosting + OAuth |
| 7 | edited `PUBLISHING.md` | crates.io section marked done, accounts table statuses, order-of-operations steps marked |
| 8 | edited `SUMMARY.md` | new §0.4 with the publish and the rendered-README proof; problems 21–23 updated |
| 9 | edited `ROADMAP.md` | Phase 4 status: published; OAuth named as the remaining gap |
| 10 | edited `ARCHITECTURE.md` | `DEPLOYMENT.md` added to the packaging-files table |

### Verification Output

The publish:

```
$ cargo publish --allow-dirty
   Packaged 51 files, 1.7MiB (1.4MiB compressed)
   Verifying mcp-obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.92s
   Uploading mcp-obsidian v0.1.0 (/home/redram/rust/mcp_obsidian)
    Uploaded mcp-obsidian v0.1.0 to registry `crates-io`
   Published mcp-obsidian v0.1.0 at registry `crates-io`
```

Live metadata, and the ownership token as the MCP Registry validator sees it:

```
$ curl -sS https://crates.io/api/v1/crates/mcp-obsidian
name       : mcp-obsidian
version    : 0.1.0 | MIT OR Apache-2.0
repo       : https://github.com/moreno-omar/mcp_obsidian

$ curl -sS -H 'Accept: application/json' …/crates/mcp-obsidian/0.1.0/readme
{"url":"https://static.crates.io/readmes/mcp-obsidian/mcp-obsidian-0.1.0.html"}

$ curl -sSL "<that url>"   # HTTP 200
token present      : True
next char          : '<'
valid boundary     : True
html comment form? : False
```

Quality gates, unchanged by any of this:

```
$ cargo clippy --all-targets     → Finished, no warnings
$ cargo test                     → 10 passed (ollama_client), 22 passed (vault_queries)
$ mcp-publisher validate server.json  → ✅ server.json is valid
```

### References Used

1. crates.io publishing — verified-email requirement, permanent versions:
   <https://doc.rust-lang.org/cargo/reference/publishing.html>
2. OpenAI plugin review requirements (public HTTPS only, domain verification,
   OAuth methods, review package):
   <https://developers.openai.com/plugins/deploy/app-review>
3. Smithery publishing (URL route needs Streamable HTTP + OAuth; MCPB route for
   local stdio): <https://smithery.ai/docs/build/publish>

### Optional steps

1. **Revoke or replace the crates.io token** at <https://crates.io/me>. Prefer a
   scoped `publish-new`/`publish-update` token; the one supplied is full-access.
2. **Commit and tag `v0.1.0`.** The release was published from a dirty tree
   (`--allow-dirty`) because committing is not automatic here; a tag is the
   provenance a CI publish would key on.
3. **Add a CI workflow** — `cargo publish` on tag plus
   `mcp-publisher login github-oidc` + `publish`, with `CARGO_REGISTRY_TOKEN` and
   `id-token: write`.
4. **List on Smithery the easy way first** — the MCPB bundle at
   `dist/mcp-obsidian-0.1.0-linux.mcpb` needs no VPS and no OAuth, so it produces a
   public listing today.
5. **Implement OAuth** (`rmcp` `auth` feature) before renting anything; it is the
   gate on both remaining listings and it is a code task, not a hosting task.
6. **Add a `/healthz` route and a systemd `WatchdogSec`** so the VPS deployment is
   monitorable once it exists.
7. **Cross-build the bundle** (`cargo-zigbuild`/`cross`) for macOS and Windows,
   and sign it, so those users do not meet Gatekeeper/SmartScreen warnings.

### Possible problems

1. **The token is still live.** Revocation is a manual step the user deferred to
   after this chat; until then `~/.cargo/credentials.toml` grants full publish
   rights to the account.
2. **Publishing from a dirty tree is a weak provenance story.** The artifact on
   crates.io corresponds to no commit. Harmless now, but it means the released
   bytes cannot be reproduced from git history.
3. **`deployment` needs the vault on the VPS.** That is a data-location change for
   a local-first tool, and it is easy to underestimate: notes can contain
   credentials, journal entries and client material.
4. **The OpenAI listing is blocked on OAuth, which does not exist yet.** No amount
   of hosting gets past the review without it, so the ordering in
   `DEPLOYMENT.md` (URL → OAuth → Smithery → OpenAI) matters.
5. **Proxy streaming config is a silent failure.** `proxy_buffering off` and long
   read timeouts are required for SSE; without them the transport appears to work
   for a small request and hangs on a real one. It is documented, but it is the
   kind of thing that gets lost when someone copies a stock nginx block.
6. **`--allow-host` must match exactly what the proxy forwards.** A mismatch is a
   hard `403` with a clear message, but only if someone reads the log; through a
   proxy the client just sees an opaque error.
7. **One version in review at a time at OpenAI.** A rejection costs a full cycle,
   so the checklists should be treated as gating rather than advisory.
8. **`mcp-obsidian` v0.1.0 is now permanent.** Any mistake in the published
   tarball cannot be deleted, only superseded by `0.1.1` — so the next release
   should still run `cargo package --list` before publishing.

## 16:42 — Note: what "v0.1.0 is permanent" actually means

Everything below is a consequence of one crates.io rule, so it is worth writing
down once rather than rediscovering it on the next release.

### The rule

The Cargo book's own wording: *"Take care when publishing a crate, because a
publish is **generally permanent**. The version can never be overwritten, and the
code cannot be deleted."* crates.io states the reason explicitly — *"One of the
major goals of crates.io is to act as a permanent archive of crates that does not
change over time, and allowing deletion of a version would go against this
goal."*

`cargo yank` is often mistaken for a delete button. It is not:

> *"A yank does not delete any code. This feature is not intended for deleting
> accidentally uploaded secrets, for example. If that happens, you must reset
> those secrets immediately."* — Cargo book

So the semantics are: **no new** dependencies can be created against a yanked
version, but **all existing** ones continue to build, the `.crate` file stays
downloadable, and the docs page stays up. Anyone with a `Cargo.lock` pinning
`=0.1.0` is unaffected. "Generally permanent" leaves room for crates.io admins to
intervene in an exceptional case (a legal or security incident), but that is not
a workflow and must not be planned around.

### The counter-intuitive part, and why it matters today

Because yanking does not remove code, **yanking is the wrong response to a leaked
secret** — the secret is still in the archived tarball and mirrored elsewhere. The
Cargo book is direct about this: you must **reset the secret immediately**. That
is the context for the `keys.md` near-miss earlier today: had it been packaged, no
amount of yanking would have undone it, and the correct action would have been
revoking the crates.io token rather than touching the release.

### What that means for `mcp-obsidian` v0.1.0 specifically

`mcp-obsidian` v0.1.0 was published on 2026-10-09 at 16:28 and is therefore
frozen. Concretely:

| Thing | Consequence |
| :- | :- |
| The source tarball | Whatever `cargo publish` packaged at that moment is what is archived. It cannot be corrected. |
| The rendered README | Fixed at publish time, which is why the `mcp-name:` token had to be right *before* publishing and cannot be injected afterwards. |
| The `name` in `Cargo.toml` | `mcp-obsidian` v0.1.0 is bound to that name; renaming the package means a new crate name, not a new version of this one. |
| The licence field | `MIT OR Apache-2.0` is recorded forever; changing it later does not change v0.1.0's metadata. |
| Download counts / dependents | Permanent and public, including any bot that mirrors the tarball within minutes of publish. |

**The one safety net is `cargo yank`**, and it is much weaker than it sounds:
it hides a version from *new* resolutions and changes nothing else.

```bash
cargo yank --version 0.1.0     # do NOT run this for a cosmetic problem
cargo yank --version 0.1.0 --undo
```

The correct response to a bad release is a corrected `0.1.1`, not a yank. A yank
is only right when the version is actively harmful (a security problem, a
squatting incident, or a build-breaking dependency), and even then it fixes
nothing that is already pinned.

### The practical consequence, stated plainly

Because `0.1.0` cannot be fixed in place, **the only defence is the check before
the upload**:

```bash
cargo package --list      # reads the real file list — nothing stray, no secrets
cargo publish --dry-run   # packages + compiles from the extracted tarball
cargo publish
```

That first command exists precisely because `Cargo.toml` uses `exclude`
(a denylist). A denylist fails open: any file dropped in the repository root is
published unless it is named. A personal `keys.md` was caught this way earlier
today, one step away from being archived permanently. It is the reason the
release flow lists that command as the important one rather than a formality.

### Why this is recorded here rather than only in TASKS.md

TASKS.md tracks work; this is a property of the registry that does not change
when the task closes. Any future release — `0.1.1`, `1.0.0` — inherits exactly
the same constraint, which is why the note lives with the logs that describe the
release rather than with the checklist that describes the next one.

**Sources for the quotes:** Cargo book, *Publishing on crates.io* —
<https://doc.rust-lang.org/cargo/reference/publishing.html> (the "permanent",
"yank", and secret-reset wording all come from that page).

**Related:** [PUBLISHING.md](../PUBLISHING.md) §1 for the release steps and §0
for the three files whose `version` fields must agree; SUMMARY.md §0.4 for what
was verified in the published artefact.
