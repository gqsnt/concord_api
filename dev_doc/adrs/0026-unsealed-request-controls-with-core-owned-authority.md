# ADR 0026 — Unsealed `RequestControls`, core-owned execution authority, timeout preference versus caps

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** none (new decision; v2.1 had no shared caller-control
  interface, call options or cancellation token)
- **Specification:** §7.2–§7.4, §8.1–§8.3 (CALL-02, CALL-03, CONTROL-01…CONTROL-03,
  BRIDGE-01)
- **Date:** 2026-09-26

## Context

Applications wrap generated requests in shared helpers ("execute any request
with our deadline and cancellation"). Those helpers need one generic bound
across many distinct generated request types, including types emitted in the
application's own crate by the macro. A sealed trait cannot be implemented by
downstream macro expansions, and a macro expansion does not acquire its
defining crate's privacy. Separately, a caller must be able to *prefer* a
timeout while an operator *caps* work, without one erasing the other.

## Decision

- `RequestControls` (`with_options`, `control_summary`, `cancel_with`) is a
  public, **unsealed** convenience trait. The library implements it on its
  terminal wrappers (`ResponseRequest`, `AcceptedBytesRequest`, `StoreRequest`,
  `CollectPrefix`, `CollectAll`, `BoundRuntimeRequest`); the macro emits
  implementations on each downstream named request. Implementing it grants no
  native request, binding, credential or executor access, and core never treats
  an implementation as authenticity evidence. Execution authority comes only
  from core-owned validated bindings behind private fields.
- Endpoint-specific wrappers are real nominal types emitted in the consuming
  crate, owning a core request object through private fields.
- **Selection versus restriction (CONTROL-01).** Each request carries its
  declared timeout, its selected timeout (`timeout(d)` replaces it,
  `reset_timeout()` restores the declaration) and independent caps from
  `CallOptions` (`cap_timeout`, `cap_sends`, `cap_replays`,
  `cap_response_bytes`, `cap_stream_lifetime`, `cap_stream_idle`). Caps
  intersect componentwise and propagate through every terminal transformation;
  a selected timeout never removes a cap.
- **Cancellation.** `CancellationToken` is a cheap cloneable handle; a request
  ORs at most eight distinct signals, deduplicated by identity, and a ninth is
  `ControlError::TooManyCancellationSources`. Unpolled requests start nothing.
- Capability methods are absent when unsupported (CALL-03); ordinary `.await`
  never paginates or stores a credential.

## Consequences

- Generic application helpers compile against downstream-generated requests
  and core terminals alike.
- Because the trait is open, core code must never trust a `RequestControls`
  implementation to be a Concord request; the compile-time and runtime
  anti-examples below guard this.
- Zero or unrepresentable durations are `ControlError`s before execution; a zero
  send cap is valid and prevents application dispatch without preparing a body
  or credentials first (CONTROL-02).

## Evidence

Obligations: V3-L14, V3-L15, V3-E18. Tests present in this tree (results: see
`IMPLEMENTATION_REPORT.md`):

- Two real crates, `tests/downstream/concord-downstream-api` and
  `tests/downstream/concord-downstream-app`; in
  `tests/downstream/concord-downstream-app/tests/downstream.rs` —
  `downstream_named_requests_implement_controls_future_and_their_own_setters`,
  `distinct_requests_and_terminals_pass_the_consumers_generic_helper`,
  `a_foreign_request_controls_implementation_gains_no_native_access`.
- `crates/concord-core/src/controls.rs` —
  `call_options_default_is_new_and_caps_intersect`,
  `zero_and_unrepresentable_durations_are_control_errors`,
  `selected_timeout_never_removes_cap`,
  `cancellation_sources_deduplicate_and_are_bounded`.
- `crates/concord/tests/runtime/journal.rs` —
  `v3_e18_caps_intersect_and_never_reset_through_terminal_wrappers`.
- `crates/concord-core/src/execute/auth_tests/store.rs` —
  `generic_request_controls_helper_executes_a_store_request`.
- `crates/concord-core/src/request/runtime_endpoint/tests.rs` —
  `bound_requests_expose_the_common_control_interface`.
- UI fixtures: `crates/concord/tests/ui/fail/privacy/foreign_request_controls.rs`,
  `crates/concord/tests/ui/fail/privacy/forged_endpoint.rs`,
  `crates/concord/tests/ui/fail/privacy/wrong_client_endpoint.rs`.

## Revisit

Do not seal the caller trait to gain authority; authority belongs to core
bindings. Additional common controls are added to the trait only when every
terminal can honor them without widening a cap.
