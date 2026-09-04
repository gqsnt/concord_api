# Concord v2.1 documentation

Concord v2.1 is a typed HTTP client contract language. It projects one client-shaped domain contract into:

- a domain-shaped generated Rust client;
- inert unbound endpoint intents;
- redaction-aware request plans;
- local and effective contract descriptions;
- one bounded managed runtime;
- stable operational evidence;
- narrow extension contracts.

## Start here

| Goal | Read |
|---|---|
| Understand the product | [Overview](overview.md) and [mental model](mental_model.md) |
| Review non-negotiable semantics | [Design invariants](design_invariants.md) |
| Declare and call a client | [Quick start](quick_start.md) |
| Model a real API | [Modeling APIs](modeling_apis.md) |
| Follow the maintained Riot example | [Riot walkthrough](riot_walkthrough.md) |
| Look up exact language meaning | [DSL specification](spec.md) |
| Understand generated Rust | [Generated client](generated_client.md) |
| Understand values and wire shaping | [Requests and values](requests_and_values.md) |
| Understand bodies and responses | [Request bodies](request_bodies.md) and [responses](responses.md) |
| Understand execution | [Execution policy](execution_policy.md) and [runtime](runtime.md) |
| Configure authentication | [Authentication](auth.md) |
| Configure retry | [Retry](retry.md) |
| Declare and enforce quotas | [Rate limits](rate_limits.md) |
| Add pagination | [Pagination](pagination.md) |
| Inspect and observe | [Planning and observation](planning_and_observation.md) |
| Build a dynamic endpoint | [Runtime endpoints](runtime_endpoints.md) |
| Implement an extension | [Extensions](extensions.md) |
| Migrate from the supplied v2.0.0 / earlier v1-style surface | [Migration guide](migration_v2_1.md) |
| Understand release evidence | [Evidence and release gates](evidence_and_release_gates.md) |

## Normative order

`design_invariants.md` and `spec.md` are normative. Subsystem documents explain the same contract by task. `dev_doc/internals.md` assigns implementation responsibility but does not override public semantics.

## Status

These documents specify the `2.1.0-beta.1` integration candidate. Publication
is disabled. The canonical lifecycle and gate state is maintained only in
[release-gate status](../dev_doc/release_gate_status.md); the claim ledger
records the evidence state behind it. No alternative public spellings are
retained as a compromise.
