# 2026-10-11 - MCPB bundle: per-platform builds in CI, released to GitHub

- **Timestamp:** 14:50

- **Objective:** Get an `.mcpb` a Claude Desktop user can download and install as
  an extension. crates.io already covers users who have Rust and Cargo; the
  bundle exists for the ones who do not. Claude Desktop's bundle support is
  **macOS and Windows only**, so a Linux-built bundle cannot serve them, and a
  Linux host cannot build a macOS binary. Goal: make the per-platform build and
  the release mechanism automatic and honest.

- **Solution:** Three files, all verified before landing.

  - **`scripts/2026-10-11_stamp-mcpb-manifest.py`** — rewrites the *staged*
    manifest so `compatibility.platforms` names only the platform the bundle
    actually carries, and fixes the Windows entry point (`server/mcp-obsidian.exe`;
    the checked-in manifest hardcodes the extension-less name). It exits non-zero
    if the entry point is not present in the bundle.
  - **`.github/workflows/release.yml`** — a four-runner matrix (linux-x64,
    darwin-arm64, darwin-x64, win32-x64). Each runner builds, stages, stamps,
    packs, then **re-reads the packed manifest and asserts `platforms == [its own
    platform]`** before uploading. A final job writes `.sha256` files and creates
    or updates the release with `GITHUB_TOKEN`.
  - **`docs/RELEASE_NOTES_0.1.0.md`** — the release body, listing one asset per
    platform plus install steps and verification.

- **Why the stamping exists:** `mcpb validate` (official CLI, v2.1.2) reports
  *"Manifest schema validation passes!"* for a manifest that advertises
  `["darwin", "win32", "linux"]` on a Linux-only bundle. The schema cannot see
  the mismatch, so nothing in the toolchain catches it. Publishing that would give
  a macOS user a file that installs and then fails to launch. The guarantee has
  to live in the build, so it does.

- **Actions Performed:**
  1. Installed the official CLI user-local, no sudo:
     `npm install --prefix <workspace>/staging/2026-10-11_mcpb-tools @anthropic-ai/mcpb`.
  2. `mcpb validate mcpb/manifest.json` → passes. `mcpb info dist/…mcpb` →
     `WARNING: Not signed` (signing is not required by anything verified so far).
  3. Wrote and tested the stamper: linux → exit 0, manifest still valid;
     win32 without a staged `.exe` → exit 1; unknown platform → exit 2.
  4. Added `aarch64-apple-darwin` and `x86_64-pc-windows-gnu` targets and ran
     `cargo check --locked --release --target …` for both, into `/tmp`, so no
     build artefacts landed in this repository. **Both failed inside
     `aws-lc-sys`, a C dependency, not in this crate's Rust.**
  5. Traced it: `cargo tree -i aws-lc-sys` →
     `mcp-obsidian → reqwest feature "default" → default-tls → rustls →
     aws-lc-rs → aws-lc-sys`. `cargo tree -e normal` confirms it is the only
     `*-sys` crate that compiles C here.
  6. Ran the patched build script end-to-end in a throwaway `/tmp` root with a
     stubbed `cargo`: stamped output for both the `zip` fallback and the official
     `mcpb pack`. Both bundles reported `platforms=['linux']`, integrity OK.
  7. Wrote and exercised the release guards: an honest Linux bundle passes; a
     Linux bundle *named* darwin is refused
     (`advertises [linux] but names darwin`); a version disagreement between
     `Cargo.toml` and `manifest.json` is refused.
  8. Landed the three files and committed them.

- **Verification Output:**
  ```
  $ mcpb validate mcpb/manifest.json      -> Manifest schema validation passes!
  $ mcpb info dist/mcp-obsidian-…mcpb     -> WARNING: Not signed

  $ cargo check --target aarch64-apple-darwin
      aws-lc-sys: cc: error: unrecognized command-line option '-arch'
  $ cargo check --target x86_64-pc-windows-gnu
      aws-lc-sys: failed to find tool "x86_64-w64-mingw32-gcc"

  $ python3 …/2026-10-11_stamp-mcpb-manifest.py <stage> linux
      stamped manifest: platforms=['linux'] entry_point='server/mcp-obsidian'
  $ python3 …/2026-10-11_stamp-mcpb-manifest.py <stage> win32
      manifest points at server/mcp-obsidian.exe, which is not in <stage>   (exit 1)

  $ bash scripts/build_mcpb.sh            # stubbed cargo, /tmp root
      stamped manifest: platforms=['linux'] entry_point='server/mcp-obsidian'
      wrote …/dist/mcp-obsidian-0.1.0-linux.mcpb   (zip path and mcpb-pack path)

  $ python3 -c "yaml.safe_load('.github/workflows/release.yml')"
      YAML OK; jobs ['build','release']; matrix 4 entries; inputs ['tag','publish']
  ```

- **References Used:**
  1. anthropics/mcpb — README and MANIFEST.md (which hosts support bundles;
     `compatibility.platforms` semantics; `mcpb pack`/`validate`): [https://github.com/anthropics/mcpb/blob/main/MANIFEST.md](https://github.com/anthropics/mcpb/blob/main/MANIFEST.md)
  2. Anthropic Support — local MCP servers on Claude Desktop (custom extension
     install path; private distribution of `.mcpb` files): [https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop)
  3. GitHub Docs — Managing releases, and events that trigger workflows
     (`workflow_dispatch` inputs, `GITHUB_TOKEN`): [https://docs.github.com/en/actions/reference/events-that-trigger-workflows](https://docs.github.com/en/actions/reference/events-that-trigger-workflows)

- **Optional steps:**
  - Run the workflow once with `publish` unticked to prove the macOS and Windows
    builds before anything is published. The release job is skipped, so a red
    build cannot publish.
  - If Windows goes red on `aws-lc-sys`, the cheap fix is dropping reqwest's
    default TLS — `reqwest = { version = "0.13", default-features = false,
    features = ["json"] }`. `reqwest` is used in exactly one file
    (`src/ollama_client/client.rs`) against a local Ollama over plain HTTP. It is
    a behaviour change (a remote HTTPS Ollama would stop working), so decide it
    rather than slipping it in.
  - Wire the stamper into `scripts/build_mcpb.sh` so local builds are honest too;
    CI calls it directly today, which is why the workflow works without that step.
  - `mcpb sign --self-signed` if the "not signed" warning matters.
  - The official directory is still a channel: `clau.de/desktop-extention-submission`
    resolves to a live form.

- **Possible problems:**
  1. **The macOS and Windows builds are unproven.** `aws-lc-sys` compiles C and
     assembly, and it is where the local cross-check failed. Runner images carry
     the toolchains, so it should build — but only a runner can settle it.
  2. **A tag push publishes immediately.** `release` has `needs: build`, so a red
     build publishes nothing, but there is no staging step between a green build
     and a public release on the tag path.
  3. **The bundles are unsigned.** `mcpb info` reports it on every artifact.
  4. **`mcpb` is assumed present on runners** via `npx --yes @anthropic-ai/mcpb`;
     if that package is renamed or yanked, the pack step fails loudly rather than
     silently producing a bad bundle.
  5. **Two local bundles still share a filename and differ by content**
     (`dist/mcp-obsidian-0.1.0-linux.mcpb` vs `~/mcp-obsidian-0.1.0-linux.mcpb`).
     The `.sha256` files CI publishes are what make a download checkable; the
     stale `$HOME` copy should be deleted so it cannot be shipped by hand.
