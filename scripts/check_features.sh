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

run_check() {
  printf '+'
  printf ' %q' "${CARGO[@]}" "$@"
  printf '\n'
  "${CARGO[@]}" "$@"
}

capture_output() {
  "${CARGO[@]}" "$@"
}

manifest_contains() {
  local label="$1"
  local file="$2"
  local needle="$3"

  if ! grep -Fq -- "$needle" "$file"; then
    echo "error: expected $label to contain: $needle" >&2
    cat "$file" >&2
    exit 1
  fi
}

manifest_not_contains() {
  local label="$1"
  local file="$2"
  local needle="$3"

  if grep -Fq -- "$needle" "$file"; then
    echo "error: expected $label to omit: $needle" >&2
    cat "$file" >&2
    exit 1
  fi
}

tree_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  output="$(capture_output tree "$@")"
  if [[ "$output" != *"$needle"* ]]; then
    echo "error: expected $label to contain: $needle" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

tree_same() {
  local label="$1"
  shift

  local default_output no_default_output
  default_output="$(capture_output tree "$@")"
  no_default_output="$(capture_output tree --no-default-features "$@")"
  if [[ "$default_output" != "$no_default_output" ]]; then
    echo "error: expected $label trees to match" >&2
    printf '%s\n' "--- default" >&2
    printf '%s\n' "$default_output" >&2
    printf '%s\n' "--- no-default" >&2
    printf '%s\n' "$no_default_output" >&2
    exit 1
  fi
}

tree_not_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  output="$(capture_output tree "$@")"
  if [[ "$output" == *"$needle"* ]]; then
    echo "error: expected $label to omit: $needle" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

expect_check_failure_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  if output="$("${CARGO[@]}" "$@" 2>&1)"; then
    echo "error: expected $label to fail" >&2
    exit 1
  fi
  if [[ "$output" != *"$needle"* ]]; then
    echo "error: expected $label failure to contain: $needle" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

run_check check -p concord_core --no-default-features
run_check check -p concord_core --no-default-features --features json
run_check check -p concord_core --no-default-features --features cache-moka
run_check check -p concord_core --no-default-features --features json,cache-moka

run_check check -p concord_macros --no-default-features
run_check check -p concord_macros

run_check check -p concord_examples --all-targets
run_check nextest run -p concord_examples

manifest_contains "concord_core Cargo.toml" "concord_core/Cargo.toml" 'default = ["rate-limit-governor"]'
manifest_not_contains "concord_macros Cargo.toml" "concord_macros/Cargo.toml" '[features]'
manifest_contains "concord_examples Cargo.toml" "concord_examples/Cargo.toml" 'default = ["cache-moka"]'

tree_contains "concord_core default feature tree" 'governor feature "default"' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'moka feature "default"' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'http-cache-semantics' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'async-compression' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'brotli' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'flate2' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'cookie_store' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'cookie ' -p concord_core --edges normal,features
tree_not_contains "concord_core no-default feature tree" 'governor feature "default"' -p concord_core --edges normal,features --no-default-features
tree_not_contains "concord_core no-default feature tree" 'moka v0.12.15' -p concord_core --edges normal,features --no-default-features
tree_not_contains "concord_core no-default feature tree" 'http-cache-semantics v3.0.0' -p concord_core --edges normal,features --no-default-features
tree_not_contains "concord_core no-default feature tree" 'serde_json v1.0.149' -p concord_core --edges normal,features --no-default-features

tree_same "concord_macros default feature tree" -p concord_macros --edges normal,features
tree_not_contains "concord_macros default feature tree" 'moka feature "default"' -p concord_macros --edges normal,features
tree_not_contains "concord_macros default feature tree" 'http-cache-semantics' -p concord_macros --edges normal,features
tree_not_contains "concord_macros default feature tree" 'moka v0.12.15' -p concord_macros --edges normal,features
tree_not_contains "concord_macros default feature tree" 'http-cache-semantics v3.0.0' -p concord_macros --edges normal,features
tree_not_contains "concord_macros default feature tree" 'serde_json v1.0.149' -p concord_macros --edges normal,features

tree_contains "concord_examples default feature tree" 'moka v0.12.15' -p concord_examples --edges normal,features
tree_contains "concord_examples default feature tree" 'http-cache-semantics v3.0.0' -p concord_examples --edges normal,features
expect_check_failure_contains \
  "concord_examples no-default build" \
  'cache default backend requires a `cache-moka` crate feature' \
  check -p concord_examples --no-default-features
