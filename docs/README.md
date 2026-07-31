# Concord documentation

- **Audience:** all readers
- **Purpose:** canonical documentation index
- **Normative owners:** `spec.md`, `runtime.md`, and `design_invariants.md`

Use the path that matches the work you are doing.

## Evaluate Concord

- [Overview](overview.md) — what problem Concord solves and what it deliberately trades away.
- [Mental model](mental_model.md) — client, scope, endpoint, values, attachments, bindings, and execution.
- [Riot walkthrough](riot_walkthrough.md) — one large API declaration read from product, author, caller, compiler, and runtime perspectives.

## Declare and use an API

- [Quick start](quick_start.md) — one coherent domain-shaped client from declaration to generated usage.
- [Modeling APIs](modeling_apis.md) — choose scopes, value owners, names, identities, and policy locations.
- [Generated client](generated_client.md) — exact mapping from declarations to client, scope, endpoint, request, controls, and terminals.
- [Requests and values](requests_and_values.md) — values, defaults, host, path, query, headers, and timeout selection.
- [Request bodies](request_bodies.md) — body role, input/spec separation, preparation, replay, pagination, streaming, and multipart.
- [Responses](responses.md) — status acceptance, buffered/streaming/discard modes, metadata, bytes, records, and errors.
- [Authentication](auth.md) — secrets, credential sources, placement, challenge, recovery, and private request boundaries.
- [Rate limits](rate_limits.md) — profiles, attachments, dimensions, keys, admission, observation, and sharing.
- [Retry](retry.md) — application retry policy, attempt bounds, classifier data, and interaction with deadlines and limiting.
- [Pagination](pagination.md) — bindings, controller state, finite prefixes, hard caps, cycle detection, and all-or-error behavior.
- [Execution policy](execution_policy.md) — one executor, attempts, deadlines, cancellation, hooks, and completion.
- [Planning and observation](planning_and_observation.md) — descriptions, safe plans, public/private information, debug, and errors.
- [Runtime endpoints](runtime_endpoints.md) — dynamic endpoint construction without a second execution path.
- [Extensions](extensions.md) — semantic extension traits, runtime configuration, trust, and finite behavior.

## Normative reference

- [DSL specification](spec.md) — exact grammar, merge rules, validation, and diagnostics.
- [Runtime model](runtime.md) — exact execution order, response modes, cancellation, observation, and configuration ownership.
- [Design invariants](design_invariants.md) — rules that implementation and extensions must preserve.

## Project and maintenance records

- [Maintainer internals](../dev_doc/internals.md)
- [Documentation contract](../dev_doc/documentation_contract.md)
- [Finalization decisions](../dev_doc/finalization_decisions.md)
- [v1 freeze declaration](../FINAL.md)
