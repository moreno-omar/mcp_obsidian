# TASKS.md - 4: To have app available in OpenAI ChatGPT App Direcotry. Have app available as Universal MCP Server (for ollama)

## Done

- [x] **confirm app is a Compliant MCP Server** — driven over stdio: `initialize`
      (protocol `2025-06-18`, `serverInfo`, `instructions`), `tools/list` (4 tools
      with JSON Schema), `tools/call`, `resources/list`, and `-32602` for an
      unknown tool. See SUMMARY.md §0.2.
- [x] **create and fill out `server.json`** — root `server.json` against schema
      `2025-12-11`, `registryType: "cargo"`, stdio transport.
      `mcp-publisher validate server.json` passes against the live registry.
- [x] **prepare the crates.io package** — dual licence (`LICENSE-MIT` +
      `LICENSE-APACHE`), description, repository, keywords, categories,
      `rust-version`, and an `exclude` list. `cargo publish --dry-run` packages
      51 files / 1.4 MiB and compiles from the tarball. The `mcp-name:` token the
      registry checks is visible in the README.
- [x] **prepare the Smithery MCPB bundle** — `mcpb/manifest.json` (validates
      against `mcpb-manifest-latest.schema.json`) plus
      `scripts/build_mcpb.sh`; the bundle builds, and the extracted binary was
      probed over stdio.
- [x] **add a Streamable-HTTP transport** — `serve --http [ADDR]` (default
      `127.0.0.1:8000/mcp`) alongside the default stdio mode, plus
      `--allow-host HOST` to admit a public hostname through `rmcp`'s
      DNS-rebinding `Host` guard. Verified over HTTP: `initialize` (200 +
      `Mcp-Session-Id`), `notifications/initialized` (202), `tools/list`,
      `tools/call`, 404 on an unknown path, 403 on a disallowed `Host`. stdio
      re-checked afterwards and unchanged.
- [x] **rename to `mcp-obsidian` and authenticate** — package and executable are
      now `mcp-obsidian` (`[[bin]]` pins the executable name so the bundle, host
      config and README cannot drift); the library crate path stays
      `mcp_obsidian`, so no Rust code changed. `cargo login` succeeded with the
      supplied token.

## Immediate blocker: crates.io email verification

- [x] **verify the email, then publish** — done 2026-10-09 16:29.
      `cargo publish --allow-dirty` uploaded successfully:
      **`mcp-obsidian` v0.1.0 is live on crates.io**
      (<https://crates.io/crates/mcp-obsidian>). The rendered README on the
      static CDN carries `mcp-name: io.github.moreno-omar/mcp-obsidian` as visible
      text with a valid trailing boundary, so the Official MCP Registry ownership
      check will pass whenever that publish is run.
- [ ] **revoke or scope the crates.io token.** Deleting `keys.md` did not revoke
      it; it is still live in `~/.cargo/credentials.toml`. Revoke at
      <https://crates.io/me> and prefer a `publish-new`/`publish-update` scoped
      token next time.

## Deferred by decision (2026-10-09)

These are ready but deliberately on hold.

- [ ] **Official MCP Registry** — `server.json` is written and
      `mcp-publisher validate server.json` passes; the publisher is installed at
      `~/.local/bin/mcp-publisher`. Its prerequisite (the crate, with the token in
      its rendered README) is now satisfied, so this is one `login github` away.
- [ ] **MCP.so** — web form at <https://mcp.so/submit?type=server>; needs a
      Google or email sign-in and $39 for the instant listing.
- [ ] **Smithery** — MCPB bundle path is ready now (no VPS, no OAuth): upload
      `dist/mcp-obsidian-0.1.0-linux.mcpb` at <https://smithery.ai/new>. The URL
      path needs the VPS and OAuth — see [DEPLOYMENT.md](DEPLOYMENT.md).

## ChatGPT / plugin directory and the HTTPS URL

Full runbook in [DEPLOYMENT.md](DEPLOYMENT.md).

- [ ] **host the HTTP transport publicly.** A VPS is the right tool, and
      `serve --http` already speaks Streamable HTTP — but the vault then has to
      live on that server, and the transport is unauthenticated until OAuth
      exists.
- [ ] **add OAuth.** Required by OpenAI's review and by Smithery's URL flow.
      `rmcp` has an `auth` feature; this is a new phase, not a config change.
- [ ] **verify the OpenAI Platform organisation** (individual or business).
- [ ] **domain verification** — serve the portal's token as plain text at
      `https://<host>/.well-known/openai-apps-challenge`.
- [ ] **submit the review package** — one version in review at a time; needs 5
      positive and 3 negative test cases, reviewer credentials without MFA, and a
      video walkthrough.
