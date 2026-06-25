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

if ! "${CARGO[@]}" nextest --version >/dev/null 2>&1; then
  echo "error: cargo-nextest is required for check_v1.sh" >&2
  echo "install with: cargo install cargo-nextest --locked" >&2
  exit 127
fi

"${CARGO[@]}" fmt --check
"${CARGO[@]}" clippy --workspace --all-targets -- -D warnings
"${CARGO[@]}" nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" "${CARGO[@]}" doc --workspace --no-deps
