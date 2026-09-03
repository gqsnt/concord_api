# Concord documentation

- **Audience:** all readers
- **Purpose:** canonical documentation index
- **Normative owners:** `spec.md`, `runtime.md`, and `design_invariants.md`

## Start here

- [Overview](overview.md) — product identity and tradeoffs.
- [Quick start](quick_start.md) — declare, build, plan, and execute a client.
- [Mental model](mental_model.md) — the declaration tree, snapshots, intents,
  and managed execution.
- [v2 architecture decision](v2_architecture.md) — projection laws, non-goals,
  and migration posture.
- [v1 to v2 migration](migration_v1_to_v2.md) — mechanical mapping and semantic
  review traps.

## Declare and use an API

- [Modeling APIs](modeling_apis.md) — ownership, values, names, and identities.
- [Generated client](generated_client.md) — builders, scopes, requests, unbound
  intents, controls, and terminals.
- [Requests and values](requests_and_values.md) — exact type law, defaults,
  route and public-head patches, and timeouts.
- [Request bodies](request_bodies.md) — truthful inputs, specs, preparation,
  replay, streams, and multipart.
- [Responses](responses.md) — truthful outputs, sealed acquisition, metadata,
  accepted bytes, and streaming.
- [Authentication](auth.md) — secrets, credential sources, placement, recovery,
  and isolation.
- [Rate limits](rate_limits.md) — profiles, canonical keys, required enforcement,
  admission, and sharing.
- [Retry](retry.md) — repeatability, dispatch knowledge, classifiers, budgets,
  and deadlines.
- [Pagination](pagination.md) — directional bindings, page limits, aggregate
  deadline, cycle detection, and bounds.
- [Execution policy](execution_policy.md) — one executor, gates, observers,
  attempts, cancellation, and evidence.
- [Planning and observation](planning_and_observation.md) — credential-free
  plans, redaction, descriptions, errors, and diagnostics.
- [Runtime endpoints](runtime_endpoints.md) — dynamic contracts through the same
  planner and executor.
- [Extensions](extensions.md) — narrow runtime variation and trust boundaries.
- [Riot walkthrough](riot_walkthrough.md) — a larger nested contract.

## Normative reference

- [DSL specification](spec.md)
- [Runtime model](runtime.md)
- [Design invariants](design_invariants.md)

## Maintainer records

- [Maintainer internals](../dev_doc/internals.md)
- [Documentation contract](../dev_doc/documentation_contract.md)
- [Finalization decisions](../dev_doc/finalization_decisions.md)
- [v2 constitution](../FINAL.md)
