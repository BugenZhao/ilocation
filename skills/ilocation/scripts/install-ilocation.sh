#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${ILOCATION_REPO_URL:-https://github.com/BugenZhao/ilocation}"
CARGO_BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to install ilocation." >&2
  echo "Install Rust first, then rerun this script." >&2
  exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "warning: ilocation is primarily intended for macOS hosts." >&2
fi

cargo install --git "$REPO_URL" --locked --force ilocation

echo "Installed ilocation to: $CARGO_BIN_DIR/ilocation"
echo "$CARGO_BIN_DIR/ilocation"
