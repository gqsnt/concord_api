# ADR 0024 — Runtime endpoints are owner-projected symbolic programs without new private authority

- **Status:** Selected (v3.0.0-beta.1)
- **Supersedes:** none (new decision; replaces the v2.1 runtime binding model,
  including `try_bind`, root bare binding and dynamic credential selection)
- **Specification:** §16.1–§16.3, §25.2 (RUNTIME-01, RUNTIME-02, BRIDGE-01,
  EXPLAIN-04)
- **Date:** 2026-09-26

## Context

Runtime endpoints exist for endpoints that cannot be declared statically. In
v2.1 the runtime builder could attach or replace authentication with an
arbitrary `Auth` value, and binding at the root did not carry the full owner
projection. A runtime endpoint could therefore differ from a generated
endpoint in origin authority, credential source or inherited shaping — the
opposite of "same executor, no authority escape".

## Decision

- `RuntimeEndpoint::builder(EndpointId, Method)` records a bounded symbolic
  operation program (path, query/header, sensitivity, body, response, status,
  effect, timeout, rate); `.response::<R, O>()` selects the output pair and
  `.build()` validates local structure. Nothing is encoded, iterated or
  prepared at construction or bind.
- Every generated client and scope handle exposes
  `bind(endpoint) -> Result<BoundRuntimeRequest<R, O>, ApiError>` and
  `plan_runtime(&endpoint)`. Binding at the root uses the same owner projection
  as binding at a scope: fixed origin, generated host labels, path, query/header
  program, authentication, timeout and every rate constraint.
- Runtime query/header operations use the static patch vocabulary and ordering
  (ADR 0015), with the exact method family of spec §16.2 (`query`,
  `query_optional`, `query_each`, `query_replace…`, `query_when_present…`,
  `query_append…`, `query_remove`, `query_replace_all`, `query_off`, the
  `header_…` equivalents, and `_sensitive`/`_via` modifiers). Structural errors
  latch at the first error.
- **No new private authority (RUNTIME-02).** A runtime endpoint inherits the
  owner's effective auth placements. It may remove them with `auth_off()` or
  remove one by its safe target identity (`auth_remove`), which the bound plan
  displays prominently. It cannot create a private target, select another
  credential source, change a challenge, reach another origin, add a raw native
  body/client, or remove an inherited rate constraint.
- Bind validates owner-level collisions, accepted/challenge overlap, generated
  endpoint-ID collisions and the explicit-limiter requirement before any work.
- No runtime pagination and no runtime `absent on` builder exist in v3; the
  typed generated surface owns traversal, and the buffered `NotFound<R>` adapter
  covers status-coded absence.

## Consequences

- v2.1 callers that selected a credential source dynamically must declare an
  explicit scope with the required `auth replace` and bind there (spec §25.2).
- Restoring the full root projection may change wire, auth or quota behavior
  for v2.1 root-bound runtime endpoints; this is a documented behavior change.
- `BoundRuntimeRequest` implements the same response and control interfaces as
  generated requests, so helpers written against `RequestControls` accept both.

## Evidence

Obligations: V3-R09, V3-R10, V3-R11, V3-S03 (runtime part), V3-S11. Tests
present in this tree (results: see `IMPLEMENTATION_REPORT.md`):

- `crates/concord/tests/runtime/runtime_endpoint.rs` —
  `runtime_endpoint_binding_to_scope_uses_effective_owner_patches_and_timeout`,
  `bound_plan_agrees_with_the_actual_tls_send_including_private_auth`,
  `runtime_scope_auth_is_inherited_and_can_be_explicitly_disabled`,
  `runtime_public_replacement_parity_matches_generated_programs`,
  `runtime_endpoints_cannot_reach_another_origin_or_private_source`,
  `inherited_rate_policies_cannot_be_removed_or_redefined`,
  `missing_limiter_cannot_bypass_bind`,
  `accepted_statuses_cannot_overlap_an_effective_challenge`,
  `generated_endpoint_id_collisions_cannot_bind`.
- `crates/concord-core/src/request/runtime_endpoint/tests.rs` —
  `construction_and_binding_never_encode_iterate_or_prepare`,
  `first_error_latch_drops_later_arguments_without_work`,
  `runtime_replacement_parity_matches_static_programs`,
  `suppressed_owner_encoders_never_run_and_survivors_run_once`,
  `protected::public_writers_cannot_target_private_auth_even_when_absent`,
  `protected::auth_removal_is_by_target_identity_and_displayed_prominently`
  (module `request::runtime_endpoint::tests::protected`, compiled with the
  `default-tls` feature; the external review asked for cited references that
  omitted `protected::` to be corrected),
  `owner_projection_is_carried_and_the_plan_matches_the_send`,
  `plan_and_bind_do_no_body_credential_limiter_or_transport_work`,
  `bound_requests_expose_the_common_control_interface`.
- UI fixtures: `crates/concord/tests/ui/fail/capabilities/runtime_endpoint_pagination_absent.rs`,
  `crates/concord/tests/ui/fail/privacy/runtime_endpoint_subdomain_absent.rs`,
  `crates/concord/tests/ui/fail/privacy/runtime_endpoint_fields_private.rs`.

## Revisit

Any future runtime capability must enter through the same owner projection and
symbolic program. Do not add an absolute-URL escape, a string-keyed `Any`
registry or a runtime credential-source selector.
