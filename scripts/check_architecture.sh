#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if command -v cargo >/dev/null 2>&1; then
  CARGO=(cargo)
elif command -v cmd.exe >/dev/null 2>&1; then
  CARGO=(cmd.exe /c cargo)
else
  echo "error: cargo not found" >&2
  exit 127
fi

if command -v rg >/dev/null 2>&1; then
  RG=(rg)
else
  RG=(grep -R -n -E)
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

section() {
  printf '\n==> %s\n' "$1"
}

fail() {
  echo "ERROR: $1" >&2
  exit 1
}

fail_with_matches() {
  local message="$1"
  local file="$2"

  echo "ERROR: $message" >&2
  echo >&2
  echo "Matching lines:" >&2
  sed -n '1,120p' "$file" >&2

  local line_count
  line_count="$(wc -l <"$file")"
  if [[ "$line_count" -gt 120 ]]; then
    echo >&2
    echo "... output truncated after 120 lines" >&2
  fi

  exit 1
}

section "concord_core dependency direction"
core_tree="$tmpdir/concord_core.tree"
"${CARGO[@]}" tree -p concord_core -e normal,build,dev --all-features >"$core_tree"
if "${RG[@]}" 'concord_macros' "$core_tree" >/dev/null 2>&1; then
  fail_with_matches "concord_core must not depend on concord_macros." "$core_tree"
fi

section "concord_core source boundary"
core_refs="$tmpdir/concord_core.refs"
if "${RG[@]}" 'concord_macros|crate::ast|Raw(Api|Ast|Client|Scope|Endpoint|Item)|NormApiTree|BehaviorProfileDef|BehaviorProfilesBlock' concord_core/src >"$core_refs" 2>/dev/null; then
  fail_with_matches "concord_core must not reference compiler-only concepts." "$core_refs"
fi

section "legacy pagination runner fence"
legacy_pagination_refs="$tmpdir/legacy_pagination.refs"
if "${RG[@]}" 'PaginationRunner::(OffsetLimit|Paged|Cursor)|Self::(OffsetLimit|Paged|Cursor)|apply_query' concord_core/src/request.rs >"$legacy_pagination_refs" 2>/dev/null; then
  fail_with_matches "concord_core/src/request.rs must not contain built-in PaginationRunner branches or apply_query." "$legacy_pagination_refs"
fi

section "pagination query-key inference fence"
pagination_inference_refs="$tmpdir/pagination_inference.refs"
if "${RG[@]}" 'infer_pagination_query_key_from_assignment|find_query_key_for_ep_field_in_ops|offset_key_from_query|limit_key_from_query|page_key_from_query|per_page_key_from_query|cursor_key_from_query' concord_macros/src >"$pagination_inference_refs" 2>/dev/null; then
  fail_with_matches "concord_macros must not infer built-in pagination query keys from endpoint operations." "$pagination_inference_refs"
fi

section "built-in pagination metadata fence"
built_in_pagination_metadata_refs="$tmpdir/built_in_pagination_metadata.refs"
if "${RG[@]}" 'PaginationPlan::OffsetLimit\s*\{[^}]*offset_key|PaginationPlan::OffsetLimit\s*\{[^}]*limit_key|PaginationPlan::Paged\s*\{[^}]*page_key|PaginationPlan::Paged\s*\{[^}]*per_page_key|PaginationPlan::Cursor\s*\{[^}]*cursor_key|PaginationPlan::Cursor\s*\{[^}]*per_page_key' concord_core/src/endpoint/plan.rs >"$built_in_pagination_metadata_refs" 2>/dev/null; then
  fail_with_matches "concord_core/src/endpoint/plan.rs must not retain built-in pagination query-key metadata." "$built_in_pagination_metadata_refs"
fi

section "codegen semantic boundary"
codegen_refs="$tmpdir/codegen.refs"
if "${RG[@]}" 'crate::ast|Raw(Api|Ast|Client|Scope|Endpoint|Item)|NormApiTree' concord_macros/src/codegen >"$codegen_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not import raw AST or normalized parser tree types." "$codegen_refs"
fi

section "codegen panic hygiene"
panic_refs="$tmpdir/panic.refs"
if "${RG[@]}" 'expect\("validated|expect\("valid|unreachable!|\.unwrap\(\)' concord_macros/src/codegen >"$panic_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not rely on validation-dependent panics." "$panic_refs"
fi

echo
echo "Architecture boundary checks passed."
