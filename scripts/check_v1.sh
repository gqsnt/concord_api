#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."

if command -v cargo >/dev/null 2>&1; then
  CARGO=(cargo)
elif command -v cmd.exe >/dev/null 2>&1; then
  CARGO=(cmd.exe /c cargo)
else
  echo "error: cargo not found" >&2
  exit 127
fi

"${CARGO[@]}" fmt --check
"${CARGO[@]}" test -p concord_core redaction
"${CARGO[@]}" test -p concord_core auth
"${CARGO[@]}" test -p concord_core cache
"${CARGO[@]}" test -p concord_core pagination
"${CARGO[@]}" test -p concord_core
"${CARGO[@]}" test -p concord_macros
"${CARGO[@]}" test -p concord_examples
"${CARGO[@]}" test --workspace
"${CARGO[@]}" doc --workspace --no-deps
"${CARGO[@]}" clippy --workspace --all-targets -- -D warnings
