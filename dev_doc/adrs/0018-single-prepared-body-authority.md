# ADR 0018 — One `PreparedBody` media authority, prepared once per operation or page

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** ADR 0006. ADR 0006's preparation-frequency rule (once per
  direct request or page, before credential and rate admission) is carried
  forward unchanged; the media and metadata authority changes.
- **Specification:** §9.2–§9.3, §9.6, §17.3, §25.2 (BODY-01, BODY-02, EXEC-03)
- **Date:** 2026-09-26

## Context

In v2.1 a request representation reported its exact `Content-Type` and length
twice: once through a metadata callback and once through preparation. The two
answers could disagree, and planning had to run the metadata callback — user
code — to explain a request. Multipart boundaries made this worse, because the
boundary is only known once it is generated. Response negotiation had the
matching problem (`fn accept()`), resolved by ADR 0016.

## Decision

- `RequestRepresentation<Input>` has a single `prepare(input, context)` method
  returning a core-owned `PreparedBody`. The prepared body is the only source of
  the actual `Content-Type`, length hint (`Unknown`, `AtLeast(n)`, `Exact(n)`)
  and mechanical replay capability. `BodyDescription` is static, bounded
  explanation metadata — never an exact header promise.
- Core constructors (`PreparedBody::empty`, `bytes`, `stream`, feature-gated
  `multipart`, and `content_type`) validate the media type once; a conflicting
  second value is an error and an identical repeat is harmless. A multipart
  boundary is generated during preparation and retained with the body.
- Planning never prepares a body (EXEC-03). A plan says that the exact media
  type or boundary is deferred to preparation.
- Preparation happens once per direct operation or page, after public shaping
  and before credential acquisition and rate admission; automatic attempts
  inside that operation reuse or reconstruct the prepared body through its
  replay capability instead of calling the representation again. An outer-absent
  optional body means no representation call, no bytes and no body-owned media
  header (BODY-02).
- The `EncodeBody`/`DecodeBody` convenience roles keep their blanket bridges; the
  request bridge calls `encode` once and applies the codec's single declared
  `CONTENT_TYPE`.
- `RequestBody` survives only as a deprecated type alias of `PreparedBody`
  for source that names the builder; it does not preserve the retired dual
  metadata obligation.

## Consequences

- Custom representations drop their metadata callback; everything the executor
  needs is on the prepared body. This is a breaking extension change listed in
  spec §25.2 ("Dual body metadata/prepare").
- A representation returning replayable bytes still does not make a POST safe
  or idempotent; replay authority stays with the effect/dispatch gates.
- Page reconstruction still requires the explicit `PageRequestRepresentation`
  promise (ADR 0017), not `Clone` alone. Body-bearing generated requests do not
  implement `Clone` (VALUE-05).

## Evidence

Obligations: V3-E05, V3-E06, V3-E07, V3-S12. Tests present in this tree
(results: see `IMPLEMENTATION_REPORT.md`):

- `crates/concord-core/src/body/prepared/tests.rs` —
  `content_type_is_validated_once_and_never_overwritten`,
  `prepared_body_kinds_lengths_and_replayability_are_core_facts`,
  `multipart_boundary_is_generated_once_and_replays_are_mechanical`,
  `dishonest_minimum_is_detected_against_observed_production`.
- `crates/concord-core/src/execute/tests/body_outcomes.rs` —
  `v3_e05_invalid_body_media_and_length_fail_before_admission_and_dispatch`,
  `v3_e05_retries_prepare_once_and_replay_or_reconstruct_mechanically`,
  `v3_e06_one_shot_stream_body_is_never_replayed`,
  `v3_e07_request_body_limits_hold_during_streaming_and_serialization`,
  `body_02_outer_absent_body_calls_no_representation_and_sends_no_media`.
- `crates/concord/tests/generated/shaping/bodies.rs` —
  `plans_invoke_no_body_credential_limiter_or_transport_work`,
  `absent_optional_bodies_invoke_no_representation_at_execution`,
  `multipart_boundary_is_generated_once_and_one_shot_bodies_never_replay`,
  `request_body_limits_apply_before_dispatch_during_streaming_and_serialization`.
- `crates/concord/tests/runtime/journal.rs` —
  `v3_e05_invalid_body_media_or_length_fails_before_admission_and_dispatch`.
- `crates/concord-extras/tests/contracts/outcomes.rs` —
  `v3_e07_downstream_encode_decode_bridges_execute_with_one_media_source`.
- UI fixture: `crates/concord/tests/ui/fail/capabilities/non_clone_body_endpoint.rs`.

## Revisit

`AggregateReusablePreparedBody` (reusing one prepared body across pages)
remains deferred, as in ADR 0006, until an explicit lifetime/replay contract and
a measurement show per-page preparation is material.
