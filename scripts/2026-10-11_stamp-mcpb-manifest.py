#!/usr/bin/env python3
"""Make one MCPB manifest tell the truth about the platform it was built for.

Why: `mcpb/manifest.json` in the repo is a single template that advertises
`"platforms": ["darwin", "win32", "linux"]`, but a bundle carries exactly one
native binary. `mcpb validate` cannot catch that — it validates the schema, not
the artifact — so a macOS user installs a Windows/Linux bundle and only finds
out when it fails to launch.

This runs on the *staged copy* inside the bundle, never on the source manifest,
and it also fixes the Windows entry point: the manifest hardcodes
`server/mcp-obsidian`, while the Windows binary is `mcp-obsidian.exe`.

Usage:
    python3 2026-10-11_stamp-mcpb-manifest.py <staged-dir> <platform>

    <staged-dir>  directory holding manifest.json and the server/ tree
    <platform>    linux | darwin | win32   (matches Node's process.platform)

Exits non-zero if the platform is unknown, or if the manifest ends up pointing
at a file that is not in the bundle.
"""

import json
import os
import sys

KNOWN_PLATFORMS = ("linux", "darwin", "win32")


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2

    stage, platform = sys.argv[1], sys.argv[2]
    if platform not in KNOWN_PLATFORMS:
        print(f"unknown platform {platform!r}; expected one of {KNOWN_PLATFORMS}", file=sys.stderr)
        return 2

    manifest_path = os.path.join(stage, "manifest.json")
    with open(manifest_path, encoding="utf-8") as handle:
        manifest = json.load(handle)

    # One binary per bundle: advertise only the platform actually carried.
    manifest.setdefault("compatibility", {})["platforms"] = [platform]

    # Windows resolves executables by extension; the Linux/macOS name has none.
    binary = "mcp-obsidian.exe" if platform == "win32" else "mcp-obsidian"
    entry_point = f"server/{binary}"
    manifest["server"]["entry_point"] = entry_point
    manifest["server"]["mcp_config"]["command"] = f"${{__dirname}}/{entry_point}"

    # The guard the toolchain does not give us: the manifest must point at a
    # file that is really here. A silent rename is how a bundle ships broken.
    if not os.path.isfile(os.path.join(stage, entry_point)):
        print(f"manifest points at {entry_point}, which is not in {stage}", file=sys.stderr)
        return 1

    with open(manifest_path, "w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=2, ensure_ascii=False)
        handle.write("\n")

    print(f"stamped manifest: platforms=[{platform!r}] entry_point={entry_point!r}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
