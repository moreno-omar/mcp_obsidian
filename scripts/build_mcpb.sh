#!/usr/bin/env bash
#
# Pack the release binary into an MCPB bundle for Smithery / Claude Desktop.
#
#   ./scripts/build_mcpb.sh
#
# Writes dist/mcp-obsidian-<version>-<platform>.mcpb: a zip holding
# manifest.json at its root and the release binary at server/mcp-obsidian.
#
# An MCPB bundle carries a native binary, so run this once per platform
# (linux, darwin, win32) you intend to publish.

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly PROJECT_ROOT
readonly BINARY="mcp-obsidian"
readonly STAGE_DIR="$PROJECT_ROOT/dist/mcpb"

## Print the `version` field from the [package] table of Cargo.toml.
package_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' "$PROJECT_ROOT/Cargo.toml" | head -n 1
}

## Print the "version" field of the MCPB manifest.
manifest_version() {
    sed -n 's/^  "version": "\(.*\)",$/\1/p' "$PROJECT_ROOT/mcpb/manifest.json" | head -n 1
}

## Map the host kernel onto the platform names MCPB uses.
host_platform() {
    case "$(uname -s)" in
        Linux) echo "linux" ;;
        Darwin) echo "darwin" ;;
        MINGW*|MSYS*|CYGWIN*) echo "win32" ;;
        *)
            echo "unsupported host: $(uname -s)" >&2
            return 1
            ;;
    esac
}

## Refuse to ship a bundle whose manifest names a different version.
check_versions_match() {
    local crate="$1" manifest="$2"
    if [ "$crate" != "$manifest" ]; then
        echo "version mismatch: Cargo.toml is $crate, mcpb/manifest.json is $manifest" >&2
        return 1
    fi
}

## Copy the manifest and the freshly built binary into the bundle layout.
stage_bundle() {
    local source_binary="$1" staged_name="$2"
    rm -rf "$STAGE_DIR"
    mkdir -p "$STAGE_DIR/server"
    cp "$PROJECT_ROOT/mcpb/manifest.json" "$STAGE_DIR/manifest.json"
    cp "$source_binary" "$STAGE_DIR/server/$staged_name"
    chmod 755 "$STAGE_DIR/server/$staged_name"
}

## Zip the staged layout; manifest.json has to sit at the archive root.
pack_bundle() {
    local bundle="$1"
    rm -f "$bundle"
    ( cd "$STAGE_DIR" && zip -q -r "$bundle" manifest.json server )
}

main() {
    local version platform source_binary staged_name bundle

    version="$(package_version)"
    check_versions_match "$version" "$(manifest_version)"
    platform="$(host_platform)"

    cargo build --release --manifest-path "$PROJECT_ROOT/Cargo.toml"

    source_binary="$PROJECT_ROOT/target/release/$BINARY"
    staged_name="$BINARY"
    if [ "$platform" = "win32" ]; then
        source_binary="$source_binary.exe"
        staged_name="$BINARY.exe"
    fi
    if [ ! -f "$source_binary" ]; then
        echo "build produced no binary at $source_binary" >&2
        return 1
    fi

    stage_bundle "$source_binary" "$staged_name"
    bundle="$PROJECT_ROOT/dist/${BINARY}-${version}-${platform}.mcpb"
    pack_bundle "$bundle"
    echo "wrote $bundle ($(du -h "$bundle" | cut -f1))"
}

main "$@"
