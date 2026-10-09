# PUBLISHING.md — releasing `mcp-obsidian` to the marketplaces

Scope: how the binary reaches crates.io and Smithery, and what the Official MCP
Registry and the ChatGPT directory each still need. **Decided 2026-10-09:** ship
to crates.io first; the Official MCP Registry, MCP.so and the ChatGPT directory
are deferred. For what the code does see [README.md](README.md); for where to
change it see [ARCHITECTURE.md](ARCHITECTURE.md).

## Accounts to register

| # | Where | Account | Cost | Status |
| :- | :- | :- | :- | :- |
| 1 | [crates.io](https://crates.io) | GitHub sign-in, verified email, API token at <https://crates.io/me> | free | **done** — `mcp-obsidian` v0.1.0 published |
| 2 | Official MCP Registry | existing GitHub account via `mcp-publisher login github` | free | deferred |
| 3 | [Smithery](https://smithery.ai/new) | GitHub (WorkOS sign-in) | free to publish | deferred |
| 4 | [MCP.so](https://mcp.so/submit?type=server) | Google or email | **$39** for instant listing | deferred |
| 5 | [Glama](https://glama.ai/mcp/servers) | GitHub / Google / email | free | auto-indexes the GitHub repo; optional |
| 6 | OpenAI Platform | verified individual or business identity | free to verify | deferred; needs public HTTPS hosting |

PulseMCP needs no action: its submission page is paused and points at the
Official MCP Registry, which it re-indexes automatically.

## 0. Bump the version in all three places

`0.1.0` appears in three files and they must agree:

| file | field |
| :- | :- |
| `Cargo.toml` | `version` |
| `server.json` | `version` (top level **and** inside `packages[0]`) |
| `mcpb/manifest.json` | `version` |

`scripts/build_mcpb.sh` refuses to build when the crate and the manifest
disagree. Nothing checks `server.json`, so check it by hand.

## 1. crates.io — done ✅

`mcp-obsidian` v0.1.0 is live: <https://crates.io/crates/mcp-obsidian>. Steps below
are for the **next** release.

**Prerequisite that is easy to miss:** crates.io refuses the *upload* if the
account has no **verified email address**, even when the token is valid. A valid
token gets you all the way to the upload and then fails with
`status 400 Bad Request: A verified email address is required to publish crates`.
It is a web step at <https://crates.io/settings/profile> and cannot be done from
the CLI. (This is exactly what blocked the first attempt.)

The crate name is **`mcp-obsidian`** (crates.io treats `mcp-obsidian` and
`mcp_obsidian` as the same name, so publishing this reserves both spellings). The
installed executable is also `mcp-obsidian`: `Cargo.toml` names it explicitly in
`[[bin]]` so the MCPB manifest, the host-config example and the README cannot
drift from it.

The README token is already in place. It has to be: the MCP Registry proves that
the crate and the registry entry are the same project by finding the literal text
`mcp-name: io.github.moreno-omar/mcp-obsidian` in the crate's **rendered** README,
and crates.io strips HTML comments, so it must be visible text. It is line 40 of
[README.md](README.md).

```bash
cargo login                      # paste the token from https://crates.io/me
cargo package --list             # READ THIS: anything stray here ships forever
cargo publish --dry-run          # packages + compiles from the tarball
cargo publish                    # uploads; versions are permanent
```

`cargo package --list` is the important line. `Cargo.toml` uses `exclude`
(a denylist), so a file dropped in the repository root is published unless it is
listed — and crates.io versions cannot be deleted. Check the list for anything
that is not source, tests, docs or fixtures.

Then confirm the installation path works end to end:

```bash
cargo install mcp-obsidian
mcp-obsidian --version
```

To confirm the token survived rendering — this is exactly what the registry does:

```bash
curl -sS -H 'Accept: application/json' \
  https://crates.io/api/v1/crates/mcp-obsidian/0.1.0/readme
# → {"url": "https://static.crates.io/readmes/.../*.html"}
curl -sSL "<that url>" | grep -o 'mcp-name: io.github.moreno-omar/mcp-obsidian'
```

## 2. Official MCP Registry — deferred

`server.json` in the repository root is already written and validated
(`mcp-publisher validate server.json` passes), so this step is ready whenever you
want it. It is deliberately **not** done yet: it must run *after* the crates.io
publish, because the registry fetches the crate README to prove ownership.

```bash
# install the publisher (linux/amd64; other builds are on the release page)
curl -sSLO https://github.com/modelcontextprotocol/registry/releases/latest/download/mcp-publisher_linux_amd64.tar.gz
tar -xzf mcp-publisher_linux_amd64.tar.gz
install -m 0755 mcp-publisher ~/.local/bin/mcp-publisher

mcp-publisher validate server.json     # ✅ server.json is valid
mcp-publisher login github             # device-code flow, opens a browser
mcp-publisher publish                  # uses ./server.json by default
```

Verify it is live:

```bash
curl -sS "https://registry.modelcontextprotocol.io/v0.1/servers?search=mcp-obsidian"
```

## 3. Smithery

Smithery accepts either a remote Streamable-HTTP URL (from `serve --http`) or a
local **MCPB** bundle. For a host that cannot run a process, upload the bundle.

```bash
./scripts/build_mcpb.sh          # → dist/mcp-obsidian-0.1.0-linux.mcpb
```

Then upload the bundle at <https://smithery.ai/new> (sign in with GitHub). Smithery
also has a CLI that publishes a bundle (`smithery mcp publish <file.mcpb> -n
<qualified-name>`); check their [publish docs](https://smithery.ai/docs/build/publish)
for the current package and the qualified name to use, since that surface changes
more often than the MCP spec does.

The bundle carries a native binary, so build one per platform you want to
support. The manifest asks the host for the vault path through `user_config`, so
no environment variable setup is needed at install time.

## 4. MCP.so

Sign in at <https://mcp.so/submit?type=server> and submit the repository URL
(`https://github.com/moreno-omar/mcp_obsidian`). The description and metadata are
derived from the repo, so the README is the listing. This step is a web form, not
a PR.

## ChatGPT / plugin directory

Full step-by-step runbook in **[DEPLOYMENT.md](DEPLOYMENT.md)** — DNS, systemd,
TLS proxy streaming config, `--allow-host`, verification commands, and the
submission checklists.

**The transport now exists; hosting and identity do not.** `serve --http` speaks
Streamable HTTP, which is what OpenAI's review requires — but the review also
demands a **publicly reachable HTTPS** endpoint and rejects local or testing
ones, then layers on OAuth, domain verification and organisation verification.
A VPS hosts it; the code changes are OAuth.

```bash
# on the host, behind a TLS-terminating reverse proxy
mcp-obsidian --vault /srv/vault serve --http 127.0.0.1:8000 --allow-host notes.example.com
```

`--allow-host` is required for any non-loopback name: `rmcp` rejects unknown
`Host` headers by default (its DNS-rebinding guard), and that is deliberate.

**There is no authentication on the HTTP transport.** Until OAuth is added, the
server is readable by anyone who can reach the proxy, so keep it loopback-only or
gate it at the proxy. `SPEC.md` still excludes multi-user hosting.

## Order of operations

1. `cargo publish` — the crate and its README must exist first. ✅ done for v0.1.0.
2. `mcp-publisher publish` (deferred) — it reads the crate README to prove
   ownership, so doing it before step 1 fails with a README-token error that looks
   like a namespace problem but is not. v0.1.0's rendered README already carries
   the token, so this prerequisite is satisfied.
3. Smithery / MCP.so / Glama — independent of the two above. Smithery's **bundle**
   path needs no VPS and no OAuth; its **URL** path does, and so does OpenAI — see
   [DEPLOYMENT.md](DEPLOYMENT.md).
