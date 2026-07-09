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

run_nextest_count_guard() {
  local label="$1"
  local min_count="$2"
  shift 2

  local tmp
  tmp="$(mktemp)"
  printf '\n==> %s\n' "$label"
  printf '+'
  printf ' %q' "$@"
  printf '\n'

  local status
  set +e
  "$@" 2>&1 | tee "$tmp"
  status=${PIPESTATUS[0]}
  set -e
  if [[ "$status" -ne 0 ]]; then
    rm -f -- "$tmp"
    return "$status"
  fi

  local count
  count="$(awk '
    /Summary/ && /tests run/ {
      for (i = 1; i <= NF; i++) {
        if ($i == "tests" && $(i + 1) == "run:") {
          print $(i - 1)
        }
      }
    }
  ' "$tmp" | tail -n 1)"
  rm -f -- "$tmp"
  if [[ -z "$count" ]]; then
    echo "error: could not parse nextest test count for $label" >&2
    exit 1
  fi
  if (( count < min_count )); then
    echo "error: $label ran $count tests, expected at least $min_count" >&2
    exit 1
  fi
  printf 'coverage guard: %s ran %s tests (minimum %s)\n' "$label" "$count" "$min_count"
}

check_public_dsl_terms() {
  local tmp
  tmp="$(mktemp)"
  set +e
  grep -RInE '^[[:space:]]*(behavior[[:space:]]+|behaviors[[:space:]]*\{|defaults[[:space:]]*\{)' \
    README.md docs concord_examples/src scripts/perf_macro_scale.sh >"$tmp"
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "error: stale public V1 DSL terminology found; use profiles/profile/default" >&2
    cat "$tmp" >&2
    rm -f -- "$tmp"
    exit 1
  fi
  rm -f -- "$tmp"
}

check_public_request_api_terms() {
  local tmp
  tmp="$(mktemp)"
  set +e
  grep -RInE 'execute_decoded_with[[:space:]]*::?<[[:space:]]*|execute_decoded_with[[:space:]]*\(|execute_raw[[:space:]]*\(\)' \
    README.md docs concord_examples/src >"$tmp"
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "error: stale public V1 request API terminology found; use response() or gated raw-response access" >&2
    cat "$tmp" >&2
    rm -f -- "$tmp"
    exit 1
  fi
  rm -f -- "$tmp"
}

check_public_dev_body_capture_terms() {
  local tmp
  tmp="$(mktemp)"
  set +e
  grep -RInE 'DevBodyCaptureConfig|dev_body_capture[[:space:]]*\(' \
    README.md docs/advanced_endpoints.md docs/design_invariants.md docs/errors.md docs/features.md docs/generated_client.md docs/quick_start.md concord_examples/src >"$tmp"
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "error: stale public dev body capture terminology found; keep it behind dangerous-dev-tools and out of the normal API" >&2
    cat "$tmp" >&2
    rm -f -- "$tmp"
    exit 1
  fi
  rm -f -- "$tmp"
}

run_step "architecture boundary" bash ./scripts/check_architecture.sh
run_step "public DSL terminology" check_public_dsl_terms
run_step "public request API" check_public_request_api_terms
run_step "public dev body capture API" check_public_dev_body_capture_terms
run_step "feature matrix" bash ./scripts/check_features.sh
run_step "format check" "${CARGO[@]}" fmt --check
# Clippy is strict in the release gate; intentional exceptions must be narrow
# item- or fixture-level allows naming the specific lint.
run_step "clippy workspace all targets" "${CARGO[@]}" clippy --workspace --all-targets -- -D warnings
run_step "clippy workspace all targets all features" "${CARGO[@]}" clippy --workspace --all-targets --all-features -- -D warnings

# Coverage baseline captured with `cargo nextest list` after raw-response tests
# were feature-gated:
# - `--workspace`: 931 tests, including macro integration/generated filtered suites.
# - `--workspace --all-features`: 953 tests, covering the all-features axis.
# - `--workspace --all-targets`: 931 tests, including trybuild_current/sema/codegen.
# Removed per-crate steps were exact subsets of these retained workspace runs.
# Removed subset commands:
# - nextest run -p concord_macros integration
# - nextest run -p concord_macros generated
# - nextest run -p concord_macros --test trybuild_current
# - nextest run -p concord_macros --test trybuild_sema
# - nextest run -p concord_macros --test trybuild_codegen
# - nextest run -p concord_core
# - nextest run -p concord_core --all-features
# - nextest run -p concord_examples
# - nextest run -p concord_examples --all-features
run_nextest_count_guard "workspace tests" 926 "${CARGO[@]}" nextest run --workspace
run_nextest_count_guard "workspace tests all features" 953 "${CARGO[@]}" nextest run --workspace --all-features
run_nextest_count_guard "workspace all-target tests" 926 "${CARGO[@]}" nextest run --workspace --all-targets
run_step "rustdoc warnings denied" env RUSTDOCFLAGS="-D warnings" "${CARGO[@]}" doc --workspace --no-deps
