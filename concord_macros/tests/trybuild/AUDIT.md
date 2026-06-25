# Trybuild audit

## Baseline

- trybuild binary: `concord_macros/tests/trybuild_current.rs`
- trybuild test functions before PR: 1 old monolithic trybuild function, removed by PR64bis
- pass fixtures before PR: 31
- fail fixtures before PR: 150
- stderr files before PR: 150
- baseline command: `cargo nextest run -p concord_macros --test trybuild_current`
- baseline wall time: 207.935s on 2026-06-25

## Policy

Trybuild fixtures are kept only when they prove one of:

- public proc-macro invocation emits intended user-facing diagnostic;
- error span points at the right user token;
- generated code compiles in a real downstream/user crate context;
- macro expansion succeeds/fails in a way only an external compile boundary proves;
- diagnostic text is part of public UX contract.

Parser, semantic, policy-merge, resolved-IR, and codegen matrix coverage belongs in fast unit/snapshot tests unless the rendered compiler diagnostic/span is the point.

Replacement references were mechanically checked with `rg` during PR64bis review; entries marked as ordinary Rust misuse intentionally do not have one-for-one replacement tests because they are not stable proc-macro diagnostics.

## Fixture table

| Fixture | Mode | Category | Decision | Reason | Replacement / existing fast coverage |
|---|---:|---|---|---|---|
| old-dsl/pass/pass_endpoint_stanza.rs -> tests/trybuild/pass/minimal_api.rs | pass | pass | KEEP_UI_PASS | Minimal macro invocation must compile in a downstream crate. | External compile boundary required. |
| old-dsl/pass/pass_fmt.rs -> tests/trybuild/pass/route_fmt.rs | pass | fmt | KEEP_UI_PASS | Public `fmt[...]` route/query/header DSL compiles at macro boundary. | External compile boundary required. |
| old-usage/pass/pass_client_config.rs -> tests/trybuild/pass/client_config.rs | pass | codegen | KEEP_UI_PASS | Generated client configuration surface compiles for a consumer. | `codegen::tests::generated_client_construction_snapshot_contains_current_api_only` covers generated tokens. |
| old-usage/pass/custom_codec_body_and_response.rs -> tests/trybuild/pass/custom_codec.rs | pass | codegen | KEEP_UI_PASS | Custom body and response codecs compile across public trait bounds. | External compile boundary required. |
| old-usage/pass/custom_pagination_controller.rs -> tests/trybuild/pass/custom_pagination.rs | pass | pagination | KEEP_UI_PASS | Custom pagination controller compiles across public trait bounds. | External compile boundary required. |
| old-usage/pass/pass_execution_pagination_auth.rs -> tests/trybuild/pass/execution_pagination_auth.rs | pass | behavior | KEEP_UI_PASS | Broad generated execution surface with auth and pagination compiles. | Runtime behavior remains in core/example integration tests. |
| old-usage/pass/pass_generated_public_api_shape.rs -> tests/trybuild/pass/public_api_shape.rs | pass | codegen | KEEP_UI_PASS | Public generated facade/request API shape compiles for consumers. | `codegen::tests::generated_facade_methods_return_core_pending_request_surface` covers token details. |
| old-dsl/fail/fail_secret_in_route_atom.rs -> tests/trybuild/fail/auth/secret_in_route_atom.rs | fail | auth | KEEP_UI_FAIL | Public secret leakage diagnostic and span on the secret token. | Span-sensitive UI contract. |
| old-usage/fail/fail_endpoint_basic_used_as_bearer.rs -> tests/trybuild/fail/auth/endpoint_basic_used_as_bearer.rs | fail | auth | KEEP_UI_FAIL | Public auth materialization mismatch diagnostic. | `sema::tests::endpoint_backed_credential_resolves_to_endpoint_output` covers valid resolution. |
| old-usage/fail/fail_reserved_credential_auth_method.rs -> tests/trybuild/fail/auth/reserved_credential_auth_method.rs | fail | auth | KEEP_UI_FAIL | Public generated auth-helper collision diagnostic and span. | Span-sensitive UI contract. |
| old-dsl/fail/fail_base_malformed_url.rs -> tests/trybuild/fail/route/base_malformed_url.rs | fail | route | KEEP_UI_FAIL | Public base URL diagnostic and span. | Retained fixture covers the user-facing route; `parse::tests::malformed_current_client_fails` covers the parser base-URL rejection path. |
| old-dsl/fail/fail_query_unknown.rs -> tests/trybuild/fail/route/query_unknown.rs | fail | route | KEEP_UI_FAIL | Public unknown endpoint-var diagnostic includes suggestion text. | `sema::tests::resolved_query_and_header_ops_preserve_order_and_optional_conditions` covers valid query IR. |
| old-dsl/fail/fail_fmt_secret_in_path.rs -> tests/trybuild/fail/fmt/fmt_secret_in_path.rs | fail | fmt | KEEP_UI_FAIL | Public secret-in-fmt diagnostic and span. | `parse::tests::fmt_secret_ref_fails` covers parser rule. |
| old-dsl/fail/fail_unknown_behavior_use.rs -> tests/trybuild/fail/policy/unknown_behavior_use.rs | fail | policy | KEEP_UI_FAIL | Representative unknown behavior/profile diagnostic. | `sema::tests::unknown_policy_profile_fails_during_resolution` covers profile-resolution failures. |
| old-dsl/fail/fail_cache_capacity_zero.rs -> tests/trybuild/fail/policy/cache_capacity_zero.rs | fail | policy | KEEP_UI_FAIL | Representative cache validation diagnostic and span. | `sema::tests::cache_sizing_fields_resolve_and_inherit` covers valid sizing. |
| old-dsl/fail/fail_retry_status_invalid.rs -> tests/trybuild/fail/policy/retry_status_invalid.rs | fail | policy | KEEP_UI_FAIL | Representative retry validation diagnostic and span. | `parse::tests::unsupported_attempts_retry_field_fails` and sema retry validation cover adjacent rules. |
| old-dsl/fail/fail_pagination_unknown_field.rs -> tests/trybuild/fail/pagination/pagination_unknown_field.rs | fail | pagination | KEEP_UI_FAIL | Public unknown pagination field diagnostic. | `sema::tests::unknown_pagination_field_fails_resolution` covers semantic reason. |
| old-usage/fail/fail_non_paginated_paginate.rs -> tests/trybuild/fail/pagination/non_paginated_paginate.rs | fail | pagination | KEEP_UI_FAIL | Generated request extension misuse is only proven at downstream compile boundary. | External compile boundary required. |
| old-usage/fail/custom_pagination_missing_default.rs -> tests/trybuild/fail/pagination/custom_pagination_missing_default.rs | fail | pagination | KEEP_UI_FAIL | Public custom pagination controller trait-bound diagnostic. | External compile boundary required. |
| old-usage/fail/body_codec_missing_trait.rs -> tests/trybuild/fail/codegen/body_codec_missing_trait.rs | fail | codegen | KEEP_UI_FAIL | Public body codec trait-bound diagnostic. | External compile boundary required. |
| old-usage/fail/response_codec_missing_trait.rs -> tests/trybuild/fail/codegen/response_codec_missing_trait.rs | fail | codegen | KEEP_UI_FAIL | Public response codec trait-bound diagnostic. | External compile boundary required. |
| old-usage/fail/fail_generated_public_type_collision.rs -> tests/trybuild/fail/codegen/generated_public_type_collision.rs | fail | codegen | KEEP_UI_FAIL | Public generated-name collision diagnostic and span. | `model::facade::tests::setter_forms_match_current_public_surface` covers facade naming primitives. |
| old-usage/fail/fail_reserved_endpoint_method.rs -> tests/trybuild/fail/codegen/reserved_endpoint_method.rs | fail | codegen | KEEP_UI_FAIL | Public reserved method collision diagnostic and span. | `codegen::tests::generated_explicit_endpoint_api_is_clean_and_matches_facade_target` covers generated surface. |
| old-dsl/pass/behavior_*.rs, old-dsl/pass/auth_group*.rs, old-dsl/pass/policies_group*.rs, old-dsl/pass/defaults_alias.rs, old-dsl/pass/pass_query_shorthand.rs, old-dsl/pass/pass_retry_default.rs, old-dsl/pass/cache_*.rs, old-dsl/pass/rate_limit_duplicate_across_layers_allowed.rs | pass | behavior/policy/auth | DELETE_DUPLICATE | These are parser/sema/pass matrix cases, not distinct external compile contracts. | `sema::tests::behavior_cache_profile_resolves_before_local_override`, `sema::tests::behavior_rate_limit_merges_with_local_rate_limit`, `sema::tests::default_behavior_applies_to_client_policy`, `sema::tests::policy_profiles_defaults_and_endpoint_overrides_resolve`, `sema::tests::auth_requirements_combine_in_client_scope_endpoint_order`, `sema::tests::cache_sizing_fields_resolve_and_inherit`, `sema::tests::duplicate_behavior_across_layers_remains_allowed`. |
| old-usage/pass/custom_codec_body.rs, old-usage/pass/custom_codec_response.rs | pass | codegen | DELETE_DUPLICATE | Covered by the combined custom codec pass fixture. | `tests/trybuild/pass/custom_codec.rs`; `codegen::tests::generated_response_snapshot_contains_decode_and_body_plan`. |
| old-usage/pass/pass_facade_navigation.rs, old-usage/pass/pass_param_builders.rs | pass | codegen | DELETE_DUPLICATE | Public facade and builder shapes are covered by broader pass fixture plus snapshots. | `tests/trybuild/pass/public_api_shape.rs`; `codegen::tests::generated_endpoint_setters_use_field_opt_and_clear_names`. |
| old-dsl/fail/fail_base_*.rs except fail_base_malformed_url.rs | fail | route | MOVE_PARSE_TEST | Duplicate base URL parser matrix. | Kept representative `tests/trybuild/fail/route/base_malformed_url.rs`; `parse::tests::malformed_current_client_fails` covers the parser base-URL rejection path. |
| old-dsl/fail/fail_endpoint_*.rs, fail_map_before_response.rs | fail | route | MOVE_PARSE_TEST | Endpoint syntax/order parser cases. | `parse::tests::endpoint_missing_response_fails`, `parse::tests::endpoint_duplicate_response_fails`, `parse::tests::endpoint_map_before_response_fails`. |
| old-dsl/fail/fail_fmt_empty.rs | fail | fmt | MOVE_PARSE_TEST | Parser-only empty fmt rejection. | `parse::tests::fmt_empty_fails`; kept span-sensitive secret-in-fmt fixture. |
| old-dsl/fail/fail_auth_in_*.rs, fail_raw_auth_*.rs, fail_secret_in_policy_expr.rs, fail_raw_secret_*.rs, fail_secret_exposure_*.rs, fail_generated_local_*_in_policy_expr.rs, fail_secret_block_in_policy_expr.rs, fail_secret_macro_in_policy_expr.rs, fail_secret_path_in_policy_expr.rs, fail_secret_in_timeout_expr.rs | fail | auth/redaction | MOVE_PARSE_TEST | Public expression-scope rejection matrix is already covered by fast parser/sema expression tests; one span-sensitive secret-route fixture remains. | `parse::tests::direct_secret_policy_expression_fails`; `sema::pagination_value_tests::pagination_expr_rejects_nested_client_vars`; `sema::pagination_value_tests::pagination_expr_rejects_nested_auth_vars`. |
| old-dsl/fail/fail_duplicate_auth_*.rs, fail_auth_group_*.rs | fail | auth | MOVE_SEMA_TEST | Duplicate auth grouping/materialization matrix. | `sema::tests::auth_requirements_combine_in_client_scope_endpoint_order`; `sema::tests::static_and_bearer_auth_credentials_resolve`. |
| old-dsl/fail/fail_behavior_*.rs, fail_behaviors_invalid_item.rs, fail_duplicate_behavior*.rs, fail_unknown_behavior_parent.rs, fail_unknown_default_behavior.rs | fail | behavior | MOVE_SEMA_TEST | Behavior merge, duplicate, inheritance, and unknown-parent matrix. | `sema::tests::behavior_cache_profile_resolves_before_local_override`, `sema::tests::behavior_rate_limit_merges_with_local_rate_limit`, `sema::tests::duplicate_behavior_across_layers_remains_allowed`. |
| old-dsl/fail/fail_policies_*.rs, fail_unknown_*_profile_*.rs | fail | policy | MOVE_SEMA_TEST | Policy/profile unknown-field and invalid-item matrix; one representative unknown behavior remains. | `sema::tests::unknown_policy_profile_fails_during_resolution`; `sema::tests::policy_profiles_defaults_and_endpoint_overrides_resolve`. |
| old-dsl/fail/fail_cache_* except fail_cache_capacity_zero.rs | fail | policy | MOVE_SEMA_TEST | Cache sizing/unit/duplicate matrix. | `sema::tests::cache_sizing_fields_resolve_and_inherit`, `sema::tests::cache_max_body_units_resolve_to_bytes`, `sema::tests::local_cache_patch_preserves_inherited_sizing_and_updates_specified_fields`. |
| old-dsl/fail/fail_rate_limit_*.rs | fail | policy | MOVE_SEMA_TEST | Rate-limit duplicate/empty/range matrix. | `sema::tests::behavior_rate_limit_key_binding_resolves_at_endpoint_attachment`, `sema::tests::default_behavior_rate_limit_requiring_endpoint_key_fails`, `sema::tests::rate_limit_observer_path_is_resolved_on_api`. |
| old-dsl/fail/fail_codec_spec_missing_type_arg.rs, fail_oauth2_token_url_invalid.rs, fail_invalid_static_header_value.rs, fail_max_attempts_zero.rs | fail | policy | MOVE_PARSE_TEST | Parser/sema literal validation cases. | `parse::tests::unsupported_attempts_retry_field_fails`; `sema::tests::retry_attempts_rejected_before_resolution`; policy parse tests cover literals. |
| old-dsl/fail/fail_fake_builtin_pagination_path.rs, fail_body_paginate.rs, fail_vars_in_pagination_expr.rs, fail_auth_in_pagination_assignment.rs, fail_secret_in_pagination_assignment.rs | fail | pagination | MOVE_SEMA_TEST | Pagination semantic matrix; one unknown-field fixture remains for public diagnostic. | `sema::tests::pagination_controllers_resolve_into_endpoint_model`, `sema::tests::unknown_pagination_field_fails_resolution`, `sema::pagination_value_tests::pagination_fmt_rejects_client_vars`. |
| old-usage/fail/custom_pagination_block.rs, fail_collect_pages.rs | fail | pagination | DELETE_DUPLICATE | Usage failure duplicates retained pagination compile-boundary failures. | `tests/trybuild/fail/pagination/non_paginated_paginate.rs`, `tests/trybuild/fail/pagination/custom_pagination_missing_default.rs`. |
| old-usage/fail/fail_endpoint_access_token_used_as_basic.rs, fail_endpoint_certificate_used_as_basic.rs, fail_endpoint_unknown_used_as_basic.rs, fail_non_credential_acquire_as.rs | fail | auth | DELETE_DUPLICATE | Auth type mismatch family represented by endpoint-basic-as-bearer and reserved helper fixtures. | `tests/trybuild/fail/auth/endpoint_basic_used_as_bearer.rs`; auth semantic validation remains covered by `sema::tests::static_and_bearer_auth_credentials_resolve` and `sema::tests::endpoint_backed_credential_resolves_to_endpoint_output`. |
| old-usage/fail/fail_duplicate_alias.rs, fail_raw_identifier_public_name.rs, fail_reserved_scope_accessor.rs | fail | codegen | DELETE_DUPLICATE | Deleted as duplicate low-value UI coverage for generated public-name validation. The public error family remains covered by retained generated-name collision fixtures. | `tests/trybuild/fail/codegen/generated_public_type_collision.rs`, `tests/trybuild/fail/codegen/reserved_endpoint_method.rs`, `model::facade::tests::setter_forms_match_current_public_surface`, `codegen::tests::generated_facade_scopes_use_clean_public_names_and_rustdoc`. |
| old-usage/fail/fail_reset_field.rs, fail_with_configure.rs, fail_missing_required_param.rs, fail_maybe_field.rs | fail | codegen | DELETE_DUPLICATE | Removed from trybuild because these are ordinary generated API misuse cases, not stable proc-macro diagnostic contracts. | Generated API shape remains covered by `codegen::tests::generated_endpoint_setters_use_field_opt_and_clear_names`, `codegen::tests::generated_client_construction_snapshot_contains_current_api_only`, and retained pass fixture `tests/trybuild/pass/public_api_shape.rs`. |

## After PR64bis

- pass fixtures after: 7
- fail fixtures after: 16
- stderr files after: 16
- full trybuild wall time after: 63.592s
- category timings:
  - pass: 7.614s
  - auth: 1.131s
  - route/fmt: 1.108s
  - policy: 1.101s
  - pagination: 1.152s
  - codegen/rustdoc: 1.197s

The category timings are warm-cache targeted runs and are noisy. The full wall time is the comparable post-change measurement for the complete trybuild binary.
