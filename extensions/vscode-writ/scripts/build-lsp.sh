#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_ROOT="$(cd "$EXT_DIR/../.." && pwd)"

BIN_DIR="$EXT_DIR/bin"
mkdir -p "$BIN_DIR"

echo "Building writ-lsp (release)..."
cargo build --release -p writ-lsp --manifest-path "$WORKSPACE_ROOT/Cargo.toml"

if [[ "$OSTYPE" == msys* || "$OSTYPE" == cygwin* ]]; then
  BINARY_NAME="writ-lsp.exe"
else
  BINARY_NAME="writ-lsp"
fi

cp "$WORKSPACE_ROOT/target/release/$BINARY_NAME" "$BIN_DIR/$BINARY_NAME"
chmod +x "$BIN_DIR/$BINARY_NAME"

echo "Copied $BINARY_NAME to $BIN_DIR"
