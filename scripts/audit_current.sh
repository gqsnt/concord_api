#!/usr/bin/env bash
set -euo pipefail

user_docs=(
  README.md
  docs/README.md
  docs/quick_start.md
  docs/mental_model.md
  docs/dsl.md
  docs/generated_client.md
  docs/auth.md
  docs/pagination.md
  docs/customization.md
  docs/cache_retry_rate_limit.md
  docs/runtime_config.md
  docs/advanced_endpoints.md
)

public_examples=(concord_examples/src)

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
    rg --glob '!target/**' --glob '!**/target/**' "$@"
  elif command -v grep >/dev/null 2>&1; then
    local pattern="$1"
    shift
    grep -R -n -E --binary-files=without-match --exclude-dir=target --exclude-dir=.git -- "$pattern" "$@"
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

require_match() {
  local label="$1"
  local pattern="$2"
  shift 2
  echo "== $label =="
  if ! run_rg "$pattern" "$@"; then
    echo "$label failed" >&2
    exit 1
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

echo "== no public versioned docs =="
if find docs -maxdepth 4 \( -iname "*v5*" -o -iname "*v6*" -o -iname "*migration*" \) | grep .; then
  echo "Public docs contain versioned/migration files" >&2
  exit 1
fi

fail_if_match \
  "old syntax in public docs/examples" \
  "part\\[|\\battempts\\b|with_configure|auth none|auth any|auth all|maybe_\\w+\\(|reset_\\w+\\(|collect_pages" \
  "${user_docs[@]}" "${public_examples[@]}"

echo "== secret namespace restricted to credential declarations =="
secret_hits="$(mktemp)"
if run_rg "secret\\." "${user_docs[@]}" "${public_examples[@]}" >"$secret_hits"; then
  if grep -E -v "credential[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=[[:space:]]*(api_key|bearer)[[:space:]]*\\([[:space:]]*secret\\." "$secret_hits"; then
    echo "secret namespace used outside credential declarations" >&2
    rm -f "$secret_hits"
    exit 1
  fi
fi
rm -f "$secret_hits"

fail_if_match \
  "internal runtime names in user-facing docs/examples" \
  "use concord_core::internal|RequestPlan|EndpointPlan|AuthPlan|RateLimitPermit|CacheKey|runtime_state" \
  "${user_docs[@]}" "${public_examples[@]}"

fail_if_match \
  "raw AST access from codegen" \
  "crate::ast|use crate::ast|crate::parse|use crate::parse" \
  concord_macros/src/codegen

fail_if_match \
  "legacy endpoint/part traits" \
  "LegacyEndpoint|RoutePart|PolicyPart|AuthPart|BodyPart|PaginationPart" \
  concord_core concord_macros

fail_if_match \
  "versioned Concord diagnostics in user-facing implementation" \
  "V6-|V5-|v6-|v5-" \
  concord_core/src concord_macros/src

fail_if_match \
  "hidden generated names in public docs/examples" \
  "__Facade|__Scope|__Endpoint" \
  "${user_docs[@]}" "${public_examples[@]}"

require_match \
  "custom codec API documented" \
  "BodyCodec|ResponseCodec" \
  docs/customization.md

require_match \
  "custom pagination API documented" \
  "PaginationController|PageRequest|PageItems|HasNextCursor" \
  docs/customization.md docs/pagination.md

fail_if_match \
  "unsupported codec registries" \
  "CodecRegistry|register_codec|register_decoder|register_encoder" \
  concord_core/src concord_macros/src docs concord_examples/src

fail_if_match \
  "custom pagination cannot mutate internal plans" \
  "fn apply\\([^\\n]*(RequestPlan|EndpointPlan)|&mut[[:space:]]+(RequestPlan|EndpointPlan)" \
  concord_core/src concord_macros/src docs concord_examples/src

echo "current audit passed"
