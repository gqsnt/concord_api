# Concord v3 documentation

Concord v3 is a Rust `api!` declaration language and managed runtime for typed
HTTP clients. One client-shaped declaration projects into:

- a domain-shaped generated Rust client with owned scope handles and named
  requests;
- inert unbound endpoint intents;
- redaction-aware bound request plans;
- a static, value-free effective contract (`Client::contract()`) and the
  versioned `concord.contract/3` inspection format;
- one bounded managed runtime with truthful execution, traversal and stream
  outcomes;
- narrow, documented extension contracts.

## Start here

| Goal | Read |
|---|---|
| Understand the product | [Overview](overview.md) and [mental model](mental_model.md) |
| Review non-negotiable semantics | [Design invariants](design_invariants.md) |
| Declare and call a client | [Quick start](quick_start.md) |
| Model a real API (the canonical Catalog) | [Modeling APIs](modeling_apis.md) |
| Follow the maintained Riot example | [Riot walkthrough](riot_walkthrough.md) |
| Look up exact language meaning | [DSL specification](spec.md) |
| Understand the generated Rust surface | [Generated client](generated_client.md) |
| Understand values and wire shaping | [Requests and values](requests_and_values.md) |
| Understand bodies | [Request bodies](request_bodies.md) |
| Understand responses, statuses and absence | [Responses](responses.md) |
| Consume byte and record streams | [Streams](streams.md) |
| Control time, caps and cancellation; read errors | [Execution policy](execution_policy.md) |
| Configure transport, runtime ceilings and limits | [Runtime](runtime.md) |
| Configure authentication and credential storage | [Authentication](auth.md) |
| Configure retry | [Retry](retry.md) |
| Declare and enforce quotas | [Rate limits](rate_limits.md) |
| Add pagination and traverse pages | [Pagination](pagination.md) |
| Inspect contracts and plans; observe operations | [Planning and observation](planning_and_observation.md) |
| Build a dynamic endpoint | [Runtime endpoints](runtime_endpoints.md) |
| Implement an extension | [Extensions](extensions.md) |
| Migrate from Concord v2.1 | [v2.1 to v3 migration guide](migration_v3.md) |
| Read the historical v2.0 to v2.1 guide | [Historical v2.1 migration guide](migration_v2_1.md) |
| Understand evidence and release gates | [Evidence and release gates](evidence_and_release_gates.md) |

## Normative order

`design_invariants.md` and `spec.md` are normative. Subsystem documents explain
the same contract by task and never define a second meaning. Where a document
and the implementation specification disagree, the specification wins and the
document is a defect. `dev_doc/internals.md` assigns implementation
responsibility but does not override public semantics.

Code in these documents is compiled: every `concord` DSL block is either a
normalized excerpt of `crates/concord-examples/src/docs_spec.rs` or an anchored
excerpt of a compiled example, and every Rust block is an anchored excerpt of
compiled code. `cargo run -p concord-xtask -- docs-check` enforces this and the
agreement of the grammar, generated names, feature matrix and migration tool
with the implementation.

## Status

These documents specify the Concord v3 unpublished beta, version
`3.0.0-beta.1`. Publication is disabled. Windows and Linux are the supported
integration platforms; macOS is not release-gated. Beta integration and stable
publication are separate decisions. The canonical lifecycle and gate state is
maintained only in [release-gate status](../dev_doc/release_gate_status.md);
the claim ledger records the evidence state behind it. No alternative public
spellings are retained as a compromise.
