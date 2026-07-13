# Test Ownership

Tests are owned by the layer whose contract they prove.

## Macro compiler

- `concord_macros/src/parse/tests/` owns grammar and raw-AST behavior.
- `concord_macros/src/sema/tests/` owns normalization, descriptors, origin
  classification, authentication, rate limits, pagination, and diagnostics.
- `concord_macros/src/codegen/tests/` owns generated-token structure.
- Trybuild pass suites own public generated surfaces; compile-fail suites prove
  removed retry DSL is rejected with client-level `RetryMode` guidance.

The macro pipeline has no retry policy IR or generated retry executor.

## Core runtime

`concord_core/tests/integration/current_core/` owns native runtime behavior:

- `retry_modes.rs` distinguishes visible `Client::execute` calls from physical
  wire requests and covers all three modes, body cloneability, and auth
  interaction;
- `runtime_order.rs` owns visible-execution ordering and bounded authentication
  recovery ordering;
- `response_body_limit.rs` owns buffered, raw, decoded, and streamed native
  response-limit behavior plus execution-path body redaction;
- `dev_body_capture.rs` owns the isolated dangerous development capture path;
- `native_runtime.rs` owns managed execution, authentication recovery, body
  construction and request limits, cancellation, and native response processing;
- `rate_limit.rs` owns acquisition, feedback, capped future-call cooldown, and
  the rule that a final 429 is not resent;
- `pagination.rs`, `redaction_matrix.rs`, `runtime_config.rs`, and
  `public_api.rs` own their named contracts.

Focused unit modules own request recipes, exact lengths, multipart,
credential-cache concurrency, cancellation, errors, and redaction primitives.

## Generated clients, examples, and performance

- `concord_macros/tests/integration/generated/` owns generated-client behavior.
- `concord_examples/tests/` owns deterministic example behavior.
- Native targets under `perf/benches/` own managed-client construction by retry
  mode, visible execution, authentication recovery, pagination, streaming,
  rate-limit cooldown, hooks, and allocation reports.

The loopback seam counts physical wire requests. Hook and rate-limiter probes
count visible executions. Tests must not infer one count from the other when
Reqwest retry is enabled.

## Validation

The root `justfile` owns workspace and release checks. `just perf-check`,
`just perf-test`, and `just bench-check` validate the excluded performance
package. Architectural removal is also enforced through compile-fail coverage
and repository searches for deleted symbols.

## P-10 deleted-file migration map

This map accounts for every test deleted with the former `runtime_order.rs` and
`attempt_pipeline.rs`. "Removed" means the assertion depended exclusively on
the deleted Concord retry executor; no production compatibility API was
restored.

### `runtime_order.rs`

| Deleted test | Disposition | Current owner |
| --- | --- | --- |
| `retry_decision_happens_before_decode` | removed because it exclusively tested deleted Concord retry behavior | removed retry-policy callback/order |
| `non_replayable_request_plan_without_stream_body_does_not_retry` | removed because it exclusively tested deleted Concord retry behavior | replaced architecture-wide by Reqwest cloneability |
| `non_replayable_request_plan_without_stream_body_does_not_auth_refresh` | covered by a named existing equivalent | `native_runtime::non_rebuildable_challenged_body_returns_original_status_without_recovery` |
| `custom_retry_policy_cannot_exceed_default_attempt_cap` | removed because it exclusively tested deleted Concord retry behavior | removed retry caps |
| `custom_retry_decision_happens_after_hook_and_rate_limit_observation` | removed because it exclusively tested deleted Concord retry behavior | removed retry callback |
| `inherited_custom_retry_policy_respects_cap_three` | removed because it exclusively tested deleted Concord retry behavior | removed inherited retry policy |
| `configured_transport_error_kind_retries_then_succeeds` | removed because it exclusively tested deleted Concord retry behavior | removed transport classifier |
| `unconfigured_transport_error_kind_does_not_retry` | removed because it exclusively tested deleted Concord retry behavior | removed transport classifier |
| `transport_error_retry_budget_exhaustion_returns_final_typed_error` | removed because it exclusively tested deleted Concord retry behavior | removed retry exhaustion/budget |
| `unsafe_method_without_idempotency_header_does_not_retry` | covered by a named existing equivalent | `retry_modes::status_mode_never_retries_an_unsafe_method` |
| `unsafe_method_with_idempotency_header_retries_with_stable_value` | removed because it exclusively tested deleted Concord retry behavior | removed idempotency-based retry eligibility |
| `rate_limit_acquire_runs_before_each_transport_attempt` | migrated to native equivalent | `runtime_order::runtime_order_auth_recovery_visible_execution_sequence` proves acquisition per visible execution; hidden sends are intentionally invisible |
| `native_request_phases_follow_auth_rate_hook_head_body_and_response_order` | migrated to native equivalent | `runtime_order::runtime_order_auth_recovery_visible_execution_sequence` and `runtime_order_success_runs_post_hook_before_rate_feedback` |
| `rate_limit_observation_runs_before_retry_decision` | removed because it exclusively tested deleted Concord retry behavior | removed retry decision |
| `runtime_hooks_observe_200_before_decode_failure` | covered by a named existing equivalent | `rate_limit::rate_limit_observes_200_before_decode_failure` plus `redaction_matrix::response_decoding_failure_redacts_response_and_request_sentinels` |
| `runtime_hooks_observe_retryable_status_before_retry` | covered by a named existing equivalent | `retry_modes::status_mode_cloneable_encoded_bytes_retry_hidden_with_identical_payload` proves the post hook sees only Reqwest's final response |
| `runtime_hooks_observe_auth_rejection_before_auth_handling` | migrated to native equivalent | `runtime_order::runtime_order_auth_recovery_visible_execution_sequence` |
| `auth_recovery_terminal_second_challenge_orders_invalidation_without_third_send` | migrated to native equivalent | `runtime_order::runtime_order_terminal_second_challenge_releases_then_invalidates_without_third_send` |
| `auth_rejection_preempts_custom_retry_policy_for_401` | removed because it exclusively tested deleted Concord retry behavior | custom retry policy removed; auth ordering retained by `runtime_order` |
| `auth_rejection_preempts_custom_retry_policy_for_403` | removed because it exclusively tested deleted Concord retry behavior | custom retry policy removed; status mode cannot select 403 |
| `never_refresh_auth_rejection_does_not_fall_through_to_custom_retry` | removed because it exclusively tested deleted Concord retry behavior | custom retry fallthrough removed |
| `auth_rejection_drops_body_without_exposing_it` | covered by a named existing equivalent | `redaction_matrix::auth_rejection_redacts_auth_sentinel_and_context` and `runtime_order` response-release checks |
| `runtime_hooks_do_not_observe_body_on_http_status_error` | covered by a named existing equivalent | `redaction_matrix::http_status_failure_redacts_request_and_response_sentinels` |
| `transport_observation_does_not_leak_basic_auth_material` | covered by a named existing equivalent | `redaction_matrix::transport_failure_redacts_request_material` and rate-limit secret-free context tests |
| `custom_retry_context_does_not_expose_bearer_auth` | removed because it exclusively tested deleted Concord retry behavior | retry context removed |
| `custom_retry_context_does_not_expose_query_auth` | removed because it exclusively tested deleted Concord retry behavior | retry context removed |
| `custom_retry_context_does_not_expose_basic_auth_material` | removed because it exclusively tested deleted Concord retry behavior | retry context removed |
| `oversized_live_body_fails_typed` | migrated to native equivalent | `response_body_limit::response_body_limit_unknown_length_exceeds_during_collection` |
| `custom_retry_policy_not_invoked_for_decode_failure` | removed because it exclusively tested deleted Concord retry behavior | retry callback removed; decode path retained by redaction tests |
| `decode_failure_does_not_consume_retry_budget` | removed because it exclusively tested deleted Concord retry behavior | retry budget removed |
| `decoded_response_exposes_user_metadata` | covered by a named existing equivalent | `output_model::decoded_response_exposes_user_metadata` |
| `direct_await_returns_decoded_value` | covered by a named existing equivalent | `output_model::direct_await_returns_decoded_value` |
| `execute_returns_same_decoded_value_as_await` | covered by a named existing equivalent | `output_model::execute_returns_same_decoded_value_as_await` |
| `execute_raw_returns_classified_raw_response` | covered by a named existing equivalent | `output_model::execute_raw_returns_classified_raw_response` |
| `execute_raw_uses_retry` | removed because it exclusively tested deleted Concord retry behavior | Concord raw retry removed |
| `per_call_overrides_apply_to_pending_request` | covered by a named existing equivalent | `runtime_config::per_request_debug_override_wins_and_does_not_leak` and per-request timeout tests |
| `decode_error_includes_endpoint_status_and_content_type` | covered by a named existing equivalent | `redaction_matrix::response_decoding_failure_redacts_response_and_request_sentinels` plus typed error accessor unit tests |
| `very_verbose_debug_does_not_emit_request_or_response_body_bytes` | migrated to native equivalent | `response_body_limit::response_body_limit_redacts_request_and_response_from_all_observers` |
| `dev_body_capture_disabled_by_default` | migrated to native equivalent | `dev_body_capture::dev_body_capture_disabled_by_default` |
| `dev_body_capture_writes_response_only_to_safe_file` | migrated to native equivalent | `dev_body_capture::dev_body_capture_writes_only_response_to_safe_file` |
| `dev_body_capture_skips_oversized_response` | migrated to native equivalent | `dev_body_capture::dev_body_capture_skips_oversized_response` |
| `dev_body_capture_skips_protected_auth_response` | migrated to native equivalent | `dev_body_capture::dev_body_capture_skips_protected_auth_response` |
| `debug_sink_body_free_when_dev_body_capture_enabled` | migrated to native equivalent | `dev_body_capture::dev_body_capture_keeps_hooks_and_debug_body_free` |
| `runtime_hooks_body_free_when_dev_body_capture_enabled` | migrated to native equivalent | `dev_body_capture::dev_body_capture_keeps_hooks_and_debug_body_free` |
| `decode_error_does_not_trigger_transport_retry` | removed because it exclusively tested deleted Concord retry behavior | transport retry classifier removed; decode error remains typed/redacted |
| `runtime_config_applies_debug_rate_limit_transport_and_pagination_loop_detection` | covered by a named existing equivalent | `runtime_config::client_config_applies_to_requests` and focused runtime-config tests |
| `response_content_length_does_not_bypass_body_limit` | migrated to native equivalent | `response_body_limit::response_body_limit_authoritative_content_length_over_limit` |
| `response_unknown_length_above_limit_fails_while_reading` | migrated to native equivalent | `response_body_limit::response_body_limit_unknown_length_exceeds_during_collection` |
| `response_exactly_at_limit_succeeds` | migrated to native equivalent | `response_body_limit::response_body_limit_exact_boundary_succeeds` |
| `buffered_response_below_limit_succeeds` | covered by a named existing equivalent | the exact-boundary test is the stronger success boundary; runtime config retains no-limit success |
| `content_length_over_limit_is_checked_while_reading` | migrated to native equivalent | `response_body_limit::response_body_limit_authoritative_content_length_over_limit` |
| `streaming_body_over_limit_rejects_during_bounded_read` | migrated to native equivalent | `response_body_limit::response_body_limit_stream_fails_before_excess_delivery` |
| `body_at_exact_limit_succeeds` | migrated to native equivalent | `response_body_limit::response_body_limit_exact_boundary_succeeds` |
| `rate_limit_response_context_remains_body_free` | migrated to native equivalent | `response_body_limit::response_body_limit_redacts_request_and_response_from_all_observers` |
| `debug_hooks_never_receive_body_bytes_on_body_errors` | migrated to native equivalent | `response_body_limit::response_body_limit_redacts_request_and_response_from_all_observers` and producer-failure test |
| `body_limit_plus_one_fails` | migrated to native equivalent | `response_body_limit::response_body_limit_plus_one_fails` |
| `decode_failure_under_limit_is_not_body_limit` | covered by a named existing equivalent | `redaction_matrix::response_decoding_failure_redacts_response_and_request_sentinels` |
| `response_too_large_does_not_decode` | migrated to native equivalent | `response_body_limit::response_body_limit_prevents_endpoint_decode` |
| `response_limit_applies` | migrated to native equivalent | `response_body_limit` boundary matrix |
| `body_limit_error_does_not_trigger_ordinary_retry` | removed because it exclusively tested deleted Concord retry behavior | ordinary retry removed; migrated limit tests prove one terminal execution |
| `execute_raw_body_limit_behavior_characterized` | migrated to native equivalent | `response_body_limit::response_body_limit_raw_and_decoded_paths_are_equivalent` |
| `request_body_bytes_remain_transport_only` | migrated to native equivalent | `response_body_limit::response_body_limit_redacts_request_and_response_from_all_observers` |
| `request_without_body_reaches_transport_as_empty` | migrated to native equivalent | `response_body_limit::response_body_limit_empty_request_body_executes_as_empty` |

### `attempt_pipeline.rs`

| Deleted test | Disposition | Current owner |
| --- | --- | --- |
| `finalized_attempt_request_is_sent_once` | covered by a named existing equivalent | `retry_modes::disabled_mode_has_one_wire_request_per_visible_execution` and managed native execution tests |
| `execute_raw_and_decoded_share_the_same_attempt_path` | migrated to native equivalent | `response_body_limit::response_body_limit_raw_and_decoded_paths_are_equivalent` plus `output_model` raw/decoded tests |
| `http_status_errors_remain_typed_and_redacted` | covered by a named existing equivalent | `redaction_matrix::http_status_failure_redacts_request_and_response_sentinels` |
| `transport_errors_remain_typed_and_redacted` | covered by a named existing equivalent | `redaction_matrix::transport_failure_redacts_request_material` |
| `replayable_encoded_bodies_can_retry_with_the_same_payload` | migrated to native equivalent | `retry_modes::status_mode_cloneable_encoded_bytes_retry_hidden_with_identical_payload` |
| `non_replayable_stream_bodies_stop_after_the_first_attempt` | covered by a named existing equivalent | `retry_modes::status_mode_does_not_resend_a_direct_stream_body` |
