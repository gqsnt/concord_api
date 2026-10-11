# ADR 0025 — Primitive-count runtime configuration setters

- **Status:** Selected (v3.0.0-beta.1), implemented (the `aggregate_max_sends`
  gap recorded earlier on 2026-09-26 is closed, see Evidence)
- **Supersedes:** none (new decision; replaces the v2.1 `NonZero*` arguments of
  the runtime builder's count and size setters)
- **Specification:** §8.3, §20.1, §20.4, §25.2 (LIMIT-01)
- **Date:** 2026-09-26

## Context

v2.1 runtime setters such as `request_body_limit(NonZeroUsize)` and
`pagination_max_pages(NonZeroU32)` made zero unrepresentable, but forced
`NonZero` construction ceremony into ordinary client setup, and a `NonZero`
constructor failure surfaced at a different place than every other
configuration error. The root builder already accumulates independent errors
into one `ClientBuildError`.

## Decision

- `configure_runtime(|r| r…)` exposes count and size setters that take
  primitive values: `request_body_limit`, `response_body_limit`,
  `stream_response_limit`, `status_body_limit`, `provider_request_limit`,
  `provider_response_limit`, `pagination_max_items`,
  `pagination_total_response_body_limit`, `pagination_identity_limit`,
  `stream_max_records`, `stream_frame_limit` (`usize`);
  `pagination_max_pages`, `provider_max_sends` (`u32`); `provider_max_origins`
  (`usize`). Durations (`runtime_endpoint_timeout`, `stream_lifetime`,
  `stream_idle_timeout`, `provider_timeout`, cooldown caps) stay `Duration`.
- An invalid value (zero, or above a §20.1 hard cap) is recorded, not replaced by
  an earlier valid selection, and reported by the final root `.build()` together
  with every other independent configuration error.
- Repeated trusted-builder selections replace the previous selected value; this
  is a configuration stage, unlike `CallOptions`, whose caps intersect.
- `disable_status_body_capture()` is the explicit way to turn status-body
  capture off; a zero body limit is not.
- `runtime_endpoint_timeout` supplies a preference only for runtime descriptions
  constructed without an owner selection; it never rewrites generated endpoint
  declarations.

## Consequences

- v2.1 call sites migrate by passing the primitive (`.get()` on an existing
  `NonZero`); the migration tool performs this rewrite (spec §25.2 row
  "Runtime body/page configuration with NonZero arguments") inside a
  `configure_runtime` closure whose receiver it proves to be a generated
  client builder, and otherwise reports the argument with
  `C3-MIGRATE-RUNTIME-NONZERO-UNRESOLVED` instead (MIGRATE-03 receiver
  identity, see the
  [decision record](../decision_record.md#migrate-03-receiver-identity-external-review-f03)).
- Validation is deferred from the argument type to the build step, so a caller
  sees all configuration mistakes at once.

## Evidence

Tests present in this tree (results: see `IMPLEMENTATION_REPORT.md`):

- `crates/concord-core/src/runtime/config.rs` —
  `limit_setters_take_primitives_and_report_invalid_latest_selection`.
- `crates/concord-core/tests/configuration/runtime_config.rs` —
  `downstream_runtime_configuration_takes_primitive_ordinary_limits`.
- UI fixture: `crates/concord/tests/ui/pass/construction/configured_client_builder.rs`.
- `crates/concord/tests/generated/pagination.rs` —
  `configured_aggregate_send_ceiling_is_intersected_with_the_call_send_cap`.

**Closed gap.** Spec §20.4 also lists `aggregate_max_sends(u32)`. It was
missing when this ADR was first written and is now implemented: the setter
takes a positive `u32` (zero is latched and reported by the root build as
`aggregate_max_sends`), the default remains `DEFAULT_TRAVERSAL_MAX_HANDOFFS`
(2,000), and the traversal intersects the selected value with
`CallOptions::cap_sends` (the call cap can only narrow it; exhaustion reports
`SendCapExhausted` when the call cap binds and `AttemptCeilingExceeded`
otherwise). Evidence: the two tests above.

## Revisit

If a setter needs a different validation stage than root build, record why in
a new ADR; do not reintroduce mandatory `NonZero` arguments for ordinary setup.
