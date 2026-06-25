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
public_macro_pass=(concord_macros/tests/trybuild/pass)

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

echo "== cargo nextest run --workspace --all-targets --all-features =="
run_cargo nextest run --workspace --all-targets --all-features

echo "== cargo clippy --workspace --all-targets --all-features -- -D warnings =="
run_cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "== RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps --all-features =="
RUSTDOCFLAGS="-D warnings" run_cargo doc --workspace --no-deps --all-features

echo "== no public versioned docs =="
if find docs -maxdepth 4 \( -iname "*v5*" -o -iname "*v6*" -o -iname "*migration*" \) | grep .; then
  echo "Public docs contain versioned/migration files" >&2
  exit 1
fi

fail_if_match \
  "old syntax in public docs/examples" \
  "part\\[|\\battempts\\b|with_configure|auth none|auth any|auth all|maybe_\\w+\\(|reset_\\w+\\(|collect_pages" \
  "${user_docs[@]}" "${public_examples[@]}"

fail_if_match \
  "public docs must not use version/migration/backcompat framing" \
  "\\bv5\\b|\\bv6\\b|migration|legacy|backwards compatibility|backward compatibility|backcompat" \
  "${user_docs[@]}"

fail_if_match \
  "split base syntax in public docs/examples/compile-pass fixtures" \
  "base +(http|https) +\"" \
  "${user_docs[@]}" "${public_examples[@]}" "${public_macro_pass[@]}"

require_match \
  "malformed base URL has compile-fail fixture" \
  "base +\"https://example\\.com/v1\"" \
  concord_macros/tests/trybuild/fail/route

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
  "ignored FacadeIr in codegen" \
  "(^|[^A-Za-z0-9])_facade_ir([^A-Za-z0-9]|$)" \
  concord_macros/src/codegen

fail_if_match \
  "facade codegen must not recompute public setter names" \
  "format!\\(\\\".*_opt|format!\\(\\\"clear_" \
  concord_macros/src/codegen/endpoints/endpoint.rs concord_macros/src/codegen/endpoints/wrapper.rs

fail_if_match \
  "legacy endpoint/part traits" \
  "LegacyEndpoint|RoutePart|PolicyPart|AuthPart|BodyPart|PaginationPart" \
  concord_core concord_macros

fail_if_match \
  "versioned Concord diagnostics in user-facing implementation" \
  "V6-|V5-|v6-|v5-" \
  concord_core/src concord_macros/src

source_version_hits="$(mktemp)"
if run_rg "(^|[^[:alnum:]_])v[0-9]+([^[:alnum:]_]|$)|(^|[^[:alnum:]_])v5([^[:alnum:]_]|$)|(^|[^[:alnum:]_])v6([^[:alnum:]_]|$)|migration|legacy|backwards compatibility|backward compatibility|backcompat" \
  concord_core/src concord_macros/src > "$source_version_hits"; then
  if grep -E -v 'path \["v1"\]|scope_path=\[Static\(\\?"v1\\?"\)\]|route\.path_mut\(\)\.push_raw\(\\?"v1\\?"\)' "$source_version_hits"; then
    echo "Versioned Concord language found in source comments/docs" >&2
    rm -f "$source_version_hits"
    exit 1
  fi
fi
rm -f "$source_version_hits"

fail_if_match \
  "hidden generated names in public docs/examples" \
  "__Facade|__Scope|__Endpoint" \
  "${user_docs[@]}" "${public_examples[@]}"

require_match \
  "custom codec API documented" \
  "BodyCodec|ResponseCodec" \
  docs/customization.md

fail_if_match \
  "old custom codec signatures" \
  "fn content_type\\(\\) -> &.static str|fn accept\\(\\) -> &.static str|fn encode\\(value: &Self::Value|fn decode\\(bytes: &Bytes" \
  concord_core/src docs concord_examples concord_macros/tests/trybuild/pass

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

fail_if_match \
  "PageItems implementors must not define item_count" \
  "fn item_count\\(&self\\)" \
  concord_core/src/pagination.rs docs/customization.md concord_examples/src/custom_pagination.rs

fail_if_match \
  "PaginationController trait must not require Default" \
  "trait PaginationController<.*Default" \
  concord_core/src

require_match \
  "custom pagination Default policy documented" \
  "Default \\+ PaginationController|must implement \`Default\`" \
  docs/customization.md

require_match \
  "custom pagination missing Default compile-fail fixture" \
  "struct HeaderCursorPagination" \
  concord_macros/tests/trybuild/fail/pagination/custom_pagination_missing_default.rs

echo "current audit passed"
