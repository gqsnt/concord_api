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

run_step() {
  local label="$1"
  shift
  printf '\n==> %s\n' "$label"
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

run_step "feature matrix" bash ./scripts/check_features.sh
run_step "format check" "${CARGO[@]}" fmt --check
run_step "clippy workspace all targets" "${CARGO[@]}" clippy --workspace --all-targets -- -D warnings
run_step "macro trybuild current" "${CARGO[@]}" nextest run -p concord_macros --test trybuild_current
run_step "macro tests" "${CARGO[@]}" nextest run -p concord_macros --test main
run_step "core tests" "${CARGO[@]}" nextest run -p concord_core
run_step "examples tests" "${CARGO[@]}" nextest run -p concord_examples
run_step "workspace all-target tests" "${CARGO[@]}" nextest run --workspace --all-targets
run_step "rustdoc warnings denied" env RUSTDOCFLAGS="-D warnings" "${CARGO[@]}" doc --workspace --no-deps
