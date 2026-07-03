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
  FILTER=(rg -v)
else
  RG=(grep -R -n -E)
  FILTER=(grep -n -E -v)
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

section "endpoint-state pagination runtime fence"
pagination_runtime_refs="$tmpdir/pagination_runtime.refs"
if "${RG[@]}" 'Pagination''Runner::(OffsetLimit|Paged|Cursor)|Self::(OffsetLimit|Paged|Cursor)|apply''_query' concord_core/src/request.rs >"$pagination_runtime_refs" 2>/dev/null; then
  fail_with_matches "concord_core/src/request.rs must not contain built-in pagination runner branches or request mutation helpers." "$pagination_runtime_refs"
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

section "built-in controller key-field fence"
built_in_controller_key_refs="$tmpdir/built_in_controller_key.refs"
if "${RG[@]}" 'pub (offset_key|limit_key|page_key|per_page_key|cursor_key):' \
  concord_core/src/pagination/offset_limit.rs \
  concord_core/src/pagination/paged.rs \
  concord_core/src/pagination/cursor.rs >"$built_in_controller_key_refs" 2>/dev/null; then
  fail_with_matches "concord_core built-in pagination controllers must not expose inert query-key fields." "$built_in_controller_key_refs"
fi

section "removed custom pagination plan fence"
custom_plan_refs="$tmpdir/custom_plan.refs"
if "${RG[@]}" 'PaginationPlan::custom|PaginationPlan :: custom|PaginationPlan::from|PaginationPlan :: from|PaginationPlan::cursor|PaginationPlan :: cursor' \
  concord_macros/src/codegen concord_macros/tests concord_examples/src docs dev_doc \
  | "${FILTER[@]}" 'contains\("PaginationPlan(::| :: )(custom|from|cursor)"\)' >"$custom_plan_refs" 2>/dev/null; then
  fail_with_matches "removed custom or built-in pagination plan output must not reappear." "$custom_plan_refs"
fi

section "removed built-in pagination plan model fence"
builtin_plan_model_refs="$tmpdir/builtin_plan_model.refs"
if "${RG[@]}" '\bPaginationPlan\b|CursorNextFn' \
  concord_core/src concord_core/tests concord_macros/src/codegen concord_macros/tests concord_examples/src docs dev_doc \
  | "${FILTER[@]}" 'contains\("PaginationPlan(::| :: )(custom|from|cursor)"\)|contains\("PaginationPlan"\)' >"$builtin_plan_model_refs" 2>/dev/null; then
  fail_with_matches "removed built-in pagination plan model must not reappear in active surfaces." "$builtin_plan_model_refs"
fi

section "macro pagination controller taxonomy fence"
controller_taxonomy_refs="$tmpdir/controller_taxonomy.refs"
if "${RG[@]}" 'PaginationControllerResolved|OffsetLimitPaginationResolved|CursorPaginationResolved|PagedPaginationResolved|PaginationControllerKind|paginate_controller_kind|validate_paginate_assignment_key|validate_cursor_pagination_controller_ty|cursor_pagination_is_exact_string|parse_cursor_flag_value' \
  concord_macros/src >"$controller_taxonomy_refs" 2>/dev/null; then
  fail_with_matches "concord_macros must not retain pagination controller taxonomy helpers." "$controller_taxonomy_refs"
fi

section "removed endpoint-state runtime layer fence"
runtime_layer_refs="$tmpdir/runtime_layer.refs"
if "${RG[@]}" 'EndpointField|EndpointPaginationController|EndpointPaginationRuntimeAdapter|EndpointPaginationRuntime|OffsetLimitBindings|PagedBindings|CursorBindings|OffsetLimitState|PagedState|CursorState|endpoint_state_pagination' \
  concord_core/src concord_macros/src/codegen concord_examples/src concord_core/tests concord_macros/tests >"$runtime_layer_refs" 2>/dev/null; then
  fail_with_matches "removed endpoint-state pagination runtime layer names must not reappear in production codegen examples or tests." "$runtime_layer_refs"
fi

section "final runtime name fence"
final_runtime_name_refs="$tmpdir/final_runtime_name.refs"
if "${RG[@]}" -n 'SingleObjectPaginationRuntime|SingleObjectPaginationRuntimeAdapter|single-object pagination runtime' \
  concord_core/src concord_core/tests concord_macros/src concord_macros/tests concord_examples/src docs dev_doc >"$final_runtime_name_refs" 2>/dev/null; then
  fail_with_matches "final pagination runtime names must stay out of active production code, tests, examples, and docs." "$final_runtime_name_refs"
fi

section "removed endpoint-state pagination syntax fence"
endpoint_state_syntax_refs="$tmpdir/endpoint_state_syntax.refs"
if "${RG[@]}" -ni 'paginate endpoint[-_]state|\bendpoint[-_]state\b' \
  concord_examples/src concord_examples/tests concord_macros/tests/trybuild/pass concord_macros/tests/snapshots docs dev_doc >"$endpoint_state_syntax_refs" 2>/dev/null; then
  fail_with_matches "removed endpoint-state pagination syntax must not reappear in active examples, pass fixtures, snapshots, or docs." "$endpoint_state_syntax_refs"
fi

section "final pagination terminology fence"
final_pagination_terms_refs="$tmpdir/final_pagination_terms.refs"
if "${RG[@]}" -ni 'bindings type|pagination[- ]plans?|controller[- ]plans?|query-key inference|query key inference|generated single_object_pagination' \
  concord_examples/src concord_examples/tests concord_macros/tests/trybuild/pass concord_macros/tests/snapshots concord_macros/src/codegen \
  docs dev_doc >"$final_pagination_terms_refs" 2>/dev/null; then
  fail_with_matches "final pagination terminology must stay out of active examples, pass fixtures, snapshots, codegen, and docs." "$final_pagination_terms_refs"
fi

section "bare built-in cursor syntax fence"
bare_cursor_refs="$tmpdir/bare_cursor.refs"
if "${RG[@]}" -n 'paginate CursorPagination[^<]' \
  concord_examples/src concord_examples/tests concord_macros/tests/trybuild/pass concord_macros/tests/snapshots docs dev_doc >"$bare_cursor_refs" 2>/dev/null; then
  fail_with_matches "bare built-in cursor pagination syntax must not appear in active examples, pass fixtures, snapshots, or docs." "$bare_cursor_refs"
fi

section "codegen semantic boundary"
codegen_refs="$tmpdir/codegen.refs"
if "${RG[@]}" 'crate::ast|Raw(Api|Ast|Client|Scope|Endpoint|Item)|NormApiTree' concord_macros/src/codegen >"$codegen_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not import raw AST or normalized parser tree types." "$codegen_refs"
fi

section "macro pagination runtime construction fence"
macro_pagination_runtime_refs="$tmpdir/macro_pagination_runtime.refs"
if "${RG[@]}" 'SingleObjectPaginationRuntimeAdapter|PaginationRuntimeAdapter|EndpointPagination\s*<' \
  concord_macros/src/codegen >"$macro_pagination_runtime_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not construct single-object pagination runtimes directly." "$macro_pagination_runtime_refs"
fi

section "codegen panic hygiene"
panic_refs="$tmpdir/panic.refs"
if "${RG[@]}" 'expect\("validated|expect\("valid|unreachable!|\.unwrap\(\)' concord_macros/src/codegen >"$panic_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not rely on validation-dependent panics." "$panic_refs"
fi

echo
echo "Architecture boundary checks passed."
