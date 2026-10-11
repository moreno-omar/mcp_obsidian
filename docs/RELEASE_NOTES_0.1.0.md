# mcp-obsidian v0.1.0

Read-only, deterministic retrieval over a local Obsidian vault, exposed to Claude
Desktop (and any MCP host) as a desktop extension.

Search by tag, by free-text term, by wikilink-graph proximity, or read a note in
full. There is no model and no network in the answer path: the same vault plus
the same query always produces the same output, and every result cites the tag,
term or link that matched it.

## Install (Claude Desktop)

1. Download the `.mcpb` for your platform from the Assets below.
2. Open it — double-click, or `Settings > Extensions > Advanced settings` →
   *Extension Developer* → **Install Extension…** and select the file.
3. When prompted, choose the Obsidian vault directory to read.

The vault is opened **read-only**; nothing is ever written back to it. The
extension asks for the vault path through `user_config`, so there is no
environment setup and no configuration file to edit.

Claude Desktop's single-click MCPB support is macOS and Windows. On Linux, use
the crate or the source instead — see below.

## Assets

One bundle per platform, each containing the native binary for that platform
only. The manifest inside each bundle advertises just that platform, so a host
cannot install a bundle it cannot run.

- `mcp-obsidian-0.1.0-darwin-arm64.mcpb` — macOS on Apple Silicon
- `mcp-obsidian-0.1.0-darwin-x64.mcpb` — macOS on Intel
- `mcp-obsidian-0.1.0-win32-x64.mcpb` — Windows x64
- `mcp-obsidian-0.1.0-linux-x64.mcpb` — Linux x64 (for Smithery and other hosts)

Verify what you downloaded:

```bash
sha256sum -c mcp-obsidian-0.1.0-<platform>-<arch>.mcpb.sha256
```

## Other ways to run it

```bash
cargo install mcp-obsidian           # crates.io
mcp-obsidian --vault ~/vault serve   # stdio, for any MCP host
```

Also listed in the Official MCP Registry as
`io.github.moreno-omar/mcp-obsidian`.

## Notes

- These bundles are **not signed**. Claude Desktop reports "not signed" on
  install; `mcpb sign` is available if that matters to you.
- The public demo endpoint at `https://mcp-obsidian.duckdns.org/mcp` serves a
  synthetic demo vault and is unauthenticated by design. Never point it at
  private notes.
- Reporting issues: https://github.com/moreno-omar/mcp_obsidian/issues

## What's in this release

<!-- Replace with the git log for the tag: git log --oneline v0.1.0 -->

- Deterministic tag / term / wikilink retrieval over an Obsidian vault
- stdio and Streamable HTTP transports
- MCPB packaging for Claude Desktop
