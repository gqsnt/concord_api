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
run_check check -p concord_core --no-default-features --features records-csv
run_check check -p concord_core --no-default-features --features json
run_check check -p concord_core --no-default-features --features transport-reqwest
run_check check -p concord_core --no-default-features --features "transport-reqwest json"
run_check check -p concord_core --all-features
run_check test -p concord_core --no-default-features no_default_rate_limit
run_check test -p concord_core --no-default-features --features json no_default_rate_limit

run_check check -p concord_macros
run_check check -p concord_macros --all-features

run_check check -p concord_examples --all-targets
run_check check -p concord_examples --all-targets --all-features


manifest_contains "concord_core Cargo.toml" "concord_core/Cargo.toml" 'default = ["rate-limit-governor", "records-csv", "transport-reqwest"]'
manifest_contains "concord_core Cargo.toml" "concord_core/Cargo.toml" 'records-csv = ["dep:csv", "dep:csv-core"]'
manifest_not_contains "concord_macros Cargo.toml" "concord_macros/Cargo.toml" '[features]'

tree_contains "concord_core default feature tree" 'governor feature "default"' -p concord_core --edges normal,features
tree_contains "concord_core default feature tree" 'reqwest v' -p concord_core --edges normal,features
tree_contains "concord_core default feature tree" 'csv v' -p concord_core --edges normal,features
tree_contains "concord_core default feature tree" 'csv-core v' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'async-compression' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'brotli' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'flate2' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'cookie_store' -p concord_core --edges normal,features
tree_not_contains "concord_core default feature tree" 'cookie ' -p concord_core --edges normal,features
tree_not_contains "concord_core no-default feature tree" 'governor feature "default"' -p concord_core --edges normal,features --no-default-features
tree_not_contains "concord_core no-default feature tree" 'reqwest v' -p concord_core --edges normal,features --no-default-features
tree_not_contains "concord_core no-default feature tree" 'csv v' -p concord_core --edges normal,features --no-default-features
tree_not_contains "concord_core no-default feature tree" 'csv-core v' -p concord_core --edges normal,features --no-default-features
tree_contains "concord_core transport-reqwest feature tree" 'reqwest v' -p concord_core --edges normal,features --no-default-features --features transport-reqwest
tree_contains "concord_core records-csv feature tree" 'csv v' -p concord_core --edges normal,features --no-default-features --features records-csv
tree_contains "concord_core records-csv feature tree" 'csv-core v' -p concord_core --edges normal,features --no-default-features --features records-csv

tree_same "concord_macros default feature tree" -p concord_macros --edges normal,features
tree_not_contains "concord_macros default feature tree" 'serde_json v' -p concord_macros --edges normal,features
