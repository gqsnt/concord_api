# ADR 0016 — Directional representation selection in v3

- **Status:** Selected (v3.0.0-beta.1); the `ACCEPT` validation gap recorded
  earlier on 2026-09-26 is closed (see Consequences)
- **Supersedes:** ADR 0001 for v3. ADR 0001's ownership decision — independent
  request/response defaults, inline exceptions and scope fences — is carried
  forward unchanged; this record restates it with the v3 selection,
  negotiation and page-eligibility changes.
- **Specification:** §3.3, §9.1, §9.4–§9.5, §10.3, §25.2 (REP-01, REP-02)
- **Date:** 2026-09-26

## Context

ADR 0001 selected directional `request R;`/`response R;` defaults at client and
scope owners, inline `via R` exceptions on body/result positions, and
`request explicit;`/`response explicit;` fences. Two v2.1 mechanisms around it
conflicted with v3's inspection and pagination laws:

- response negotiation was a user function (`fn accept()`), so a static
  description or plan had to run extension code to learn the `Accept` value;
- page eligibility used a separate `PageResponseRepresentation` marker, which
  in practice tied pageability to an iterable output rather than to the page
  controller that actually extracts items.

## Decision

- **Selection by direction.** Request and response defaults are independent;
  the closest default wins for its direction. A scope fence removes the
  inherited default for descendants until another default or an inline `via`
  supplies one. Inline body/result `via` wins only in its direction. Endpoints
  have no block-level default syntax. A bodyless endpoint needs no request
  representation; every result needs an effective response representation.
- **Selection by trait obligation (REP-01).** The macro emits obligations
  (`RequestRepresentation<Input>`, `ResponseRepresentation<Output>`) and never
  infers a representation from result-type tokens such as `Vec`, `Option` or an
  alias.
- **Static negotiation.** `ResponseRepresentation` declares
  `const ACCEPT: Option<&'static str>`; descriptions and plans read the
  constant without calling converter code. An invalid `ACCEPT` (for example one
  with injected line breaks) never reaches a request: a runtime descriptor
  rejects it at `build()`, and a generated endpoint rejects it when its request
  head is prepared for a plan or an execution. `NotFound<R>` forwards
  `R::ACCEPT`.
- **Driver-based page eligibility (REP-02).** The `PageResponseRepresentation`
  gate is removed. A page type is eligible when its representation uses the
  core-sealed `Buffered` driver and a compatible controller knows how to
  extract items (ADR 0017), so a buffered JSON envelope is pageable.
- **Core-sealed drivers.** `Buffered`, `Streaming` and `DiscardDriver` remain
  the only native acquisition modes; `DiscardDriver` drops the accepted body
  without polling it and is never described as draining.

## Consequences

- Custom response representations migrate `fn accept()` to the associated
  constant (spec §25.2 row "Response `fn accept()`").
- Json requires a valid `application/json` or `application/*+json` response
  media type on its accepted branch and reports `InvalidMediaType` before body
  conversion.
- Inherited representation remains a hidden dependency; the effective
  selection and its fence/override provenance are part of the static contract
  description and of `concord.contract/3` exports.
- **Closed gap.** Spec §9.5 validates `ACCEPT` once at client build or runtime
  bind. The macro now emits, once per generated endpoint,
  `const _: () = __private::assert_response_accept(<R as ResponseRepresentation<O>>::ACCEPT);`
  (`crates/concord-macros/src/emit/endpoint.rs`), so an invalid static value is
  a compile error of the `api!` expansion — earlier than client build, and no
  client can exist with it. Runtime endpoints reject it in
  `RuntimeEndpointBuilder::build`. Head preparation still converts the
  already-validated constant into a header value. Evidence: UI fixture
  `crates/concord/tests/ui/fail/capabilities/invalid_static_accept.rs` and
  `static_accept_values_are_validated_once_at_compile_or_bind_time`
  (`crates/concord-core/src/execute/tests/body_outcomes.rs`).

## Evidence

Obligations: V3-O02 (static `ACCEPT` export), V3-P01 (envelope without an
iterable bound). Tests present in this tree (results: see
`IMPLEMENTATION_REPORT.md`):

- `crates/concord-macros/src/semantic/tests.rs` —
  `representations_are_selected_by_direction_with_explicit_fences`.
- `crates/concord/tests/generated/representation_inheritance.rs` —
  `inheritance_override_and_fence_provenance_are_public_contract_facts`,
  `parent_representation_diff_reports_caller_media_and_acquisition_changes`.
- `crates/concord-core/src/inspection/tests.rs` —
  `representation_fences_block_inheritance_and_are_named`,
  `static_accept_and_body_facts_are_read_without_converter_calls`.
- `crates/concord-inspect/tests/generated_contract.rs` —
  `generated_static_accept_is_exported_without_converter_calls`.
- `crates/concord-core/src/execute/tests/body_outcomes.rs` —
  `static_accept_values_are_validated_once_at_compile_or_bind_time`,
  `discard_driver_drops_the_accepted_body_without_polling`.
- `crates/concord/tests/generated/pagination.rs` —
  `downstream_envelope_controller_pages_a_json_envelope_without_iterable_bounds`.
- UI fixtures: `crates/concord/tests/ui/fail/syntax/request_fence_requires_inline_representation.rs`,
  `crates/concord/tests/ui/fail/syntax/response_fence_requires_inline_representation.rs`,
  `crates/concord/tests/ui/fail/syntax/endpoint_accept_removed.rs`,
  `crates/concord/tests/ui/fail/capabilities/generated_accept_override_absent.rs`,
  `crates/concord/tests/ui/fail/capabilities/runtime_accept_override_absent.rs`.

## Revisit

As in ADR 0001: if authors cannot locate the effective representation or pair
diagnostics stay remote, narrow inheritance rather than adding aliases. Human
comprehension of inherited representation remains unmeasured.
