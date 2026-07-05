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

if ! "${CARGO[@]}" deny --version >/dev/null 2>&1; then
  echo "error: cargo-deny is required for check_supply_chain.sh" >&2
  echo "install with: cargo install cargo-deny --locked" >&2
  exit 127
fi

run_step() {
  local label="$1"
  shift
  printf '\n==> %s\n' "$label"
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run_step "cargo deny advisories bans licenses sources" "${CARGO[@]}" deny check advisories bans licenses sources
