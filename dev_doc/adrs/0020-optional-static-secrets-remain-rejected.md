# ADR 0020 — Optional static secrets remain rejected in v3

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** ADR 0010's disposition. ADR 0010 deferred optional static
  secrets "for v2.1"; v3 makes the rejection a selected contract of this major
  version instead of a pending deferral. ADR 0010's invariant (a declared
  placement with unavailable material fails before dispatch and never drops
  authentication) is carried forward unchanged.
- **Specification:** §11.1–§11.2, §26.2 (AUTH-01)
- **Date:** 2026-09-26

## Context

ADR 0010 rejected `secret name?;` in v2.1 because public optionality does not
define a private authority lifecycle, and named two supported alternatives. The
v3 specification re-examined the credential model (ADR 0021) and kept the same
source kinds: each `secret` is a required protected builder input, a `custom`
provider is a required builder input whose acquisition may report unavailable,
and an `endpoint` source starts empty. No representative multi-auth API or
owner decision requiring optional static material was supplied.

## Decision

- The v3 grammar accepts only required `secret NAME;`. The postfix optional
  form (for example `secret optional_api_key?;`) is rejected with a focused
  `[C3-AUTH]` diagnostic that names the alternatives: an endpoint-backed source
  (initially empty) or a custom provider that may report unavailable.
- A declared placement whose material is missing, expired or unobtainable fails
  before application dispatch; authentication is never silently omitted.
- Endpoint-backed slots are never populated implicitly: a challenge on an
  endpoint-backed placement does not invoke the login endpoint.

## Consequences

- Clients that need an unauthenticated and an authenticated surface declare an
  explicit scope with `auth replace`/`auth off`, or use an endpoint-backed slot
  that starts empty; runtime endpoints cannot select another source (ADR 0024).
- The rejection is part of the v3 negative space, not an unimplemented
  feature.

## Evidence

Obligations: V3-E11 (unusable material fails before send). Tests present in
this tree (results: see `IMPLEMENTATION_REPORT.md`):

- UI fixture: `crates/concord/tests/ui/fail/syntax/optional_static_secret_deferred.rs`
  with its expected diagnostic in `optional_static_secret_deferred.stderr`
  (the fixture file keeps its v2.1 name; its content asserts the v3 message).
- `crates/concord-core/src/execute/auth_tests/source_recovery.rs` —
  `unusable_material_fails_before_application_dispatch_for_every_source`,
  `endpoint_backed_challenge_is_final_and_never_invokes_its_source`.
- `crates/concord/tests/generated/authentication.rs` —
  `v3_e11_missing_expired_and_unobtainable_material_fails_before_application_send`,
  `v3_e11_endpoint_backed_challenge_never_invokes_its_login_endpoint`.

## Revisit

Only with a representative multi-auth API, an explicit owner decision and a
prototype of the complete private-state lifecycle. Punctuation alone is not a
capability model.
