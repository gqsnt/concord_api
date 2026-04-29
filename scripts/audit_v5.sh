#!/usr/bin/env bash
set -euo pipefail

public_docs=(docs)
canonical_examples=(concord_examples/src/v5 concord_examples/src)

run_cargo() {
  if [[ -n "${CARGO:-}" ]]; then
    "$CARGO" "$@"
  elif command -v cargo >/dev/null 2>&1 && cargo --version >/dev/null 2>&1; then
    cargo "$@"
  elif command -v cmd.exe >/dev/null 2>&1; then
    cmd.exe /C cargo "$@"
  else
    echo "cargo not found; set CARGO=/path/to/cargo and rerun" >&2
    exit 127
  fi
}

run_rg() {
  if command -v rg >/dev/null 2>&1 && rg --version >/dev/null 2>&1; then
    rg "$@"
  elif command -v grep >/dev/null 2>&1; then
    local pattern="$1"
    shift
    grep -R -n -E -- "$pattern" "$@"
  else
    echo "rg/grep not found; install ripgrep or run from a shell with grep" >&2
    exit 127
  fi
}

fail_if_match() {
  local label="$1"
  local pattern="$2"
  shift 2
  echo "== $label =="
  if run_rg "$pattern" "$@"; then
    echo "$label failed" >&2
    exit 1
  fi
}

echo "== cargo fmt --check =="
run_cargo fmt --check

echo "== cargo test --all-features =="
run_cargo test --all-features

echo "== cargo test -p concord_examples --all-features =="
run_cargo test -p concord_examples --all-features

echo "== cargo clippy --all-targets --all-features -- -D warnings =="
run_cargo clippy --all-targets --all-features -- -D warnings

echo "== cargo clippy -p concord_examples --all-targets --all-features -- -D warnings =="
run_cargo clippy -p concord_examples --all-targets --all-features -- -D warnings

echo "== cargo doc --no-deps --all-features =="
run_cargo doc --no-deps --all-features

bad_docs="migr""ation|Migr""ation|depre""cated|rename""d|removed in ""v5|old syn""tax"
fail_if_match "non-v5 docs wording" "$bad_docs" "${public_docs[@]}"
bad_dsl="scheme"":|host"":|use_""auth|backoff ""none|response ""custom|route""\.host|part""\[|\battempt""s\b"
fail_if_match "non-v5 DSL tokens in public docs/examples" "$bad_dsl" "${public_docs[@]}" "${canonical_examples[@]}"
bad_macro="Client""Def|Layer""Def|Endpoint""Def|Auth""Block|Unsupported""AllGroup|Unsupported""AnyGroup|AuthUseKind::""Custom|AuthCredentialKind::""Custom|Legacy""Syntax"
fail_if_match "non-v5 macro model names" "$bad_macro" concord_macros/src concord_macros/tests concord_examples/tests
bad_runtime="Legacy""Endpoint|Route""Part|Policy""Part|Auth""Part|Body""Part|Pagination""Part|Auth""Controller|Auth""Chain|OneOf""Auth|Use""Credential|execute_decoded_ref_""with"
fail_if_match "legacy runtime symbols" "$bad_runtime" concord_core/src concord_macros/src concord_examples/src concord_examples/tests
fail_if_match "codegen raw AST boundary" 'crate::ast|use crate::ast' concord_macros/src/codegen
fail_if_match "internal imports in public docs/examples" 'use concord_core::internal' "${public_docs[@]}" "${canonical_examples[@]}"

echo "== explicit endpoint usage audit =="
run_rg 'api\.request\(' "${canonical_examples[@]}" "${public_docs[@]}" || true

echo "== auth session canonical usage =="
run_rg 'acquire_as_' "${canonical_examples[@]}" "${public_docs[@]}"

echo "v5 audit passed"
