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

section "macro pagination controller taxonomy fence"
controller_taxonomy_refs="$tmpdir/controller_taxonomy.refs"
if "${RG[@]}" 'PaginationControllerResolved|OffsetLimitPaginationResolved|CursorPaginationResolved|PagedPaginationResolved|PaginationControllerKind|paginate_controller_kind|validate_paginate_assignment_key|validate_cursor_pagination_controller_ty|cursor_pagination_is_exact_string|parse_cursor_flag_value' \
  concord_macros/src >"$controller_taxonomy_refs" 2>/dev/null; then
  fail_with_matches "concord_macros must not retain pagination controller taxonomy helpers." "$controller_taxonomy_refs"
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

section "generated-code private surface fence"
generated_private_refs="$tmpdir/generated_private.refs"
if ! "${RG[@]}" '__private::' concord_macros/src/codegen >/dev/null 2>&1; then
  fail "concord_macros codegen must target concord_core::__private for generated-only internals."
fi
if "${RG[@]}" '::concord_core::internal::' concord_macros/src/codegen >"$generated_private_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not emit concord_core::internal for generated-only internals." "$generated_private_refs"
fi

section "macro pagination runtime construction fence"
macro_pagination_runtime_refs="$tmpdir/macro_pagination_runtime.refs"
if "${RG[@]}" 'SingleObjectPaginationRuntimeAdapter|PaginationRuntimeAdapter|EndpointPagination\s*<' \
  concord_macros/src/codegen >"$macro_pagination_runtime_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not construct pagination runtimes directly." "$macro_pagination_runtime_refs"
fi

section "dangerous surface feature gate fence"
dangerous_surface_refs="$tmpdir/dangerous_surface.refs"
if ! "${RG[@]}" 'pub mod dangerous' concord_core/src/lib.rs >/dev/null 2>&1; then
  fail "concord_core/src/lib.rs must expose a dangerous surface module."
fi
if ! "${RG[@]}" '#\[cfg\(feature = "dangerous-raw-response"\)\]' concord_core/src/lib.rs >/dev/null 2>&1; then
  fail "concord_core/src/lib.rs must gate raw-response exports behind dangerous-raw-response."
fi
if ! "${RG[@]}" '#\[cfg\(feature = "dangerous-dev-tools"\)\]' concord_core/src/lib.rs >/dev/null 2>&1; then
  fail "concord_core/src/lib.rs must gate dev-tool exports behind dangerous-dev-tools."
fi

section "macro request body construction fence"
macro_request_body_refs="$tmpdir/macro_request_body.refs"
if "${RG[@]}" 'BodyPlan::(Encoded|RawStream|Records|Multipart|None)|RequestArgs::(with_body_bytes|with_stream_body|with_record_body|with_multipart_body|default)|BodyCodec::encode|try_content_type' \
  concord_macros/src/codegen/endpoints/endpoint.rs >"$macro_request_body_refs" 2>/dev/null; then
  fail_with_matches "concord_macros request-body planning must flow through RequestEntity adapters." "$macro_request_body_refs"
fi

section "codegen I/O entity metadata fence"
codegen_io_family_refs="$tmpdir/codegen_io_family.refs"
if "${RG[@]}" 'ResolvedRequestBodyIo|ResolvedResponseBodyIo' \
  concord_macros/src/codegen >"$codegen_io_family_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must use entity metadata, not sema syntax-family classifications." "$codegen_io_family_refs"
fi

macro_endpoint_plan_refs="$tmpdir/macro_endpoint_plan.refs"
if "${RG[@]}" 'BodyPlan::|RequestArgs::' \
  concord_macros/src/codegen/endpoints >"$macro_endpoint_plan_refs" 2>/dev/null; then
  fail_with_matches "concord_macros endpoint codegen must not construct core request plans or arguments directly." "$macro_endpoint_plan_refs"
fi

section "macro response plan construction fence"
macro_response_plan_refs="$tmpdir/macro_response_plan.refs"
if "${RG[@]}" 'ResponsePlan \{|ResponsePlan\.decode|PlanDecodeFn|ResponseCodec>::try_accept|ResponseCodec>::decode|decode : __decode_|decode: __decode_|endpoint_response_decode_fn|endpoint_response_accept_tokens|endpoint_response_no_content_tokens|endpoint_response_format_tokens' \
  concord_macros/src/codegen >"$macro_response_plan_refs" 2>/dev/null; then
  fail_with_matches "concord_macros response planning must flow through ResponseEntity adapters." "$macro_response_plan_refs"
fi

section "legacy response decode plumbing fence"
legacy_response_decode_refs="$tmpdir/legacy_response_decode.refs"
if "${RG[@]}" 'ResponsePlan\.decode|PlanDecodeFn|Box\s*<\s*dyn\s+(std::any::)?Any\s*\+\s*Send|std::any::Any|downcast_response|downcast::<DecodedResponse' \
  concord_core/src concord_macros/src/codegen docs dev_doc >"$legacy_response_decode_refs" 2>/dev/null; then
  fail_with_matches "legacy response decode plumbing must not reappear in active core, codegen, or docs surfaces." "$legacy_response_decode_refs"
fi

section "pagination response codec fence"
pagination_execution="$tmpdir/pagination_execution.rs"
sed -n '/pub async fn collect/,/^fn validate_collect_termination/p' \
  concord_core/src/request.rs >"$pagination_execution"
pagination_codec_refs="$tmpdir/pagination_codec.refs"
if "${RG[@]}" 'ResponseCodec|execute_plan::<E::Response>' \
  "$pagination_execution" >"$pagination_codec_refs" 2>/dev/null; then
  fail_with_matches "pagination must execute decoded pages through Endpoint::execute without requiring page values to be response codecs." "$pagination_codec_refs"
fi

section "decoded value response codec fence"
decoded_value_codec_refs="$tmpdir/decoded_value_codec.refs"
if "${RG[@]}" 'impl ResponseCodec for (String|Bytes|\(\)|User|PaginationItems|MatchIds)' \
  concord_core/src concord_core/tests concord_examples concord_macros/tests >"$decoded_value_codec_refs" 2>/dev/null; then
  fail_with_matches "decoded values and domain models must not implement ResponseCodec; endpoint adapters own decoding." "$decoded_value_codec_refs"
fi

section "macro streaming response execution fence"
macro_response_exec_refs="$tmpdir/macro_response_exec.refs"
if "${RG[@]}" 'execute_plan_stream|execute_plan_records|execute_plan_multipart|execute_plan_sse' \
  concord_macros/src/codegen/endpoints/endpoint.rs >"$macro_response_exec_refs" 2>/dev/null; then
  fail_with_matches "concord_macros streaming response execution must flow through ResponseEntity adapters." "$macro_response_exec_refs"
fi

section "specialized response helper public fence"
specialized_response_helper_refs="$tmpdir/specialized_response_helper.refs"
if "${RG[@]}" 'execute_plan_stream|execute_plan_records|execute_plan_multipart|execute_plan_sse' \
  concord_core/src concord_macros/src/codegen docs dev_doc >"$specialized_response_helper_refs" 2>/dev/null; then
  fail_with_matches "specialized response execution helper names must not appear in public or generated surfaces." "$specialized_response_helper_refs"
fi

section "macro streaming marker trait fence"
macro_stream_marker_refs="$tmpdir/macro_stream_marker.refs"
if "${RG[@]}" 'StreamResponseEndpoint|RecordResponseEndpoint|MultipartResponseEndpoint|SseResponseEndpoint' \
  concord_macros/src/codegen/endpoints/endpoint.rs >"$macro_stream_marker_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not reference legacy streaming marker traits." "$macro_stream_marker_refs"
fi

section "legacy streaming trait fence"
legacy_streaming_traits_refs="$tmpdir/legacy_streaming_traits.refs"
if "${RG[@]}" 'StreamResponseEndpoint|RecordResponseEndpoint|MultipartResponseEndpoint|SseResponseEndpoint|StreamResponseKind|RecordResponseKind|MultipartResponseKind|SseResponseKind|mod response_kind|trait Sealed' \
  concord_core/src concord_macros/src/codegen docs dev_doc concord_core/tests concord_macros/tests >"$legacy_streaming_traits_refs" 2>/dev/null; then
  fail_with_matches "legacy streaming marker traits and response-kind routing traits must not reappear in production code, docs, or tests." "$legacy_streaming_traits_refs"
fi

section "entity codegen positive fence"
entity_codegen_refs="$tmpdir/entity_codegen.refs"
"${RG[@]}" 'RequestEntity>::prepare|ResponseEntity>::plan|ResponseEntity>::execute' \
  concord_macros/src/codegen/endpoints/endpoint.rs >"$entity_codegen_refs" 2>/dev/null || true
if ! "${RG[@]}" 'RequestEntity>::prepare' "$entity_codegen_refs" >/dev/null 2>&1; then
  fail "concord_macros endpoint codegen must prepare request bodies through RequestEntity."
fi
if ! "${RG[@]}" 'ResponseEntity>::plan' "$entity_codegen_refs" >/dev/null 2>&1; then
  fail "concord_macros endpoint codegen must plan responses through ResponseEntity."
fi
if ! "${RG[@]}" 'ResponseEntity>::execute' "$entity_codegen_refs" >/dev/null 2>&1; then
  fail "concord_macros endpoint codegen must execute responses through ResponseEntity."
fi

section "codegen panic hygiene"
panic_refs="$tmpdir/panic.refs"
if "${RG[@]}" 'expect\("validated|expect\("valid|unreachable!|\.unwrap\(\)' concord_macros/src/codegen >"$panic_refs" 2>/dev/null; then
  fail_with_matches "concord_macros codegen must not rely on validation-dependent panics." "$panic_refs"
fi

echo
echo "Architecture boundary checks passed."
