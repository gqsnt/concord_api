#!/usr/bin/env bash
set -euo pipefail

normal_docs=(
  docs/00-quick-start.md
  docs/01-mental-model.md
  docs/02-dsl-overview.md
  docs/03-generated-usage.md
  docs/04-runtime-config.md
  docs/05-auth.md
  docs/06-pagination.md
  docs/07-cache-retry-rate-limit.md
  docs/16-dsl-reference.md
)

canonical_examples=(
  concord_examples/src/v5
)

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

echo "== cargo fmt --check =="
run_cargo fmt --check

echo "== cargo test --all-features =="
run_cargo test --all-features

echo "== cargo clippy --all-targets --all-features -- -D warnings =="
run_cargo clippy --all-targets --all-features -- -D warnings

echo "== cargo doc --no-deps --all-features =="
run_cargo doc --no-deps --all-features

echo "== old DSL syntax in canonical examples / normal docs =="
if run_rg 'scheme:|host:|use_auth|backoff none|response custom|route\.host|part\[' \
  "${canonical_examples[@]}" "${normal_docs[@]}"; then
  echo "old DSL syntax leaked into canonical v5 examples or normal docs" >&2
  exit 1
fi

echo "== old formatting/retry syntax in production v5 surfaces =="
if run_rg 'part\[|\battempts\b' \
  concord_core/src concord_macros/src/codegen "${canonical_examples[@]}" "${normal_docs[@]}"; then
  echo "old formatting/retry syntax leaked into production v5 surfaces" >&2
  exit 1
fi

echo "== legacy runtime symbols =="
if run_rg 'LegacyEndpoint|RoutePart|PolicyPart|AuthPart|BodyPart|PaginationPart|AuthController|AuthChain|OneOfAuth|UseCredential|execute_decoded_ref_with' \
  concord_core/src concord_macros/src/codegen concord_examples/src; then
  echo "legacy runtime symbol leaked into production code" >&2
  exit 1
fi

echo "== codegen raw AST boundary =="
if run_rg 'crate::ast|ClientDef|LayerDef|EndpointDef|AuthBlock|RetryProfilesBlock|CacheProfilesBlock|RateLimitProfilesBlock|LegacySyntax' \
  concord_macros/src/codegen; then
  echo "codegen depends on raw AST or legacy parser model" >&2
  exit 1
fi

echo "== internal leakage in canonical examples / normal docs =="
if run_rg 'use concord_core::internal' "${canonical_examples[@]}" "${normal_docs[@]}"; then
  echo "internal import leaked into canonical examples or normal docs" >&2
  exit 1
fi

if run_rg 'RequestPlan|EndpointPlan|AuthPlan|CredentialSlot|RateLimitPermit|CacheKey|runtime_state' \
  "${canonical_examples[@]}" "${normal_docs[@]}"; then
  echo "advanced/internal type leaked into canonical examples or normal docs" >&2
  exit 1
fi

echo "== explicit endpoint usage audit =="
run_rg 'api\.request\(' "${canonical_examples[@]}" "${normal_docs[@]}" || true

echo "== auth session canonical usage =="
run_rg 'acquire_as_' "${canonical_examples[@]}" docs

echo "v5 audit passed"
