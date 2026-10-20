# Documentation contract

**Status:** Active documentation truth rules for `3.0.0-beta.1`. The
vocabulary lists below include the forms v3 retired from v2.1.

## Truth rules

1. Documentation describes implemented behavior in the branch where it lives.
2. Selected target behavior not yet implemented is marked explicitly and should remain in the implementation kit or feature branch, not active release docs.
3. `docs/design_invariants.md`, `docs/spec.md`, and `docs/execution_policy.md` are the highest normative in-repository documents.
4. Generated Rustdoc, static descriptions, semantic diff, and hand-written docs consume the same vocabulary and semantic graph.
5. Every full canonical API declaration shown as recommended usage is
   source-anchored to compiled code.
6. Every normative Rust usage fence is source-anchored to compiled code.
7. `concord-pseudocode` is the explicit machine-visible label for partial
   grammar fragments, alternatives, deliberately incomplete excerpts, and
   explanatory state tables only. Surrounding prose names the omitted context.
8. A claim that depends on Rust coherence, performance, user cognition, or operation remains provisional until its gate passes.
9. No doc calls a plan “safe,” a local limiter “distributed,” a timeout “operation-wide,” or a response “complete” unless the exact guarantee is implemented.
10. Secrets, body values, sensitive public values, and raw rate keys never appear in example diagnostic output.
11. `dev_doc/release_gate_status.md` is the only lifecycle/gate owner. Historical
   plans, audits, and exact-SHA evidence must identify their limited scope.

## Vocabulary

Use consistently:

```text
client contract
scope snapshot
endpoint contract / endpoint intent
bound request
logical operation
pagination aggregate
page operation
visible attempt
transport-private physical recovery
request/response representation
effect: safe | idempotent | non_idempotent
body replayability
credential source / slot / material / placement / challenge / recovery
rate policy / limiter
RequestPlan
static/effective description
OperationObserver
ByteStream / ItemStream<T> / RequestStream<T>
```

Avoid old canonical vocabulary outside migration history:

```text
vars, ep, generic scope reference, host, fmt
ResponseBodySpec, response_bytes, SafeRequestPlan
timeout off, broad hook, repeatability never/idempotent as complete effect
```

Retired by v3 (use only in `docs/migration_v3.md`, negative fixtures and
history):

```text
exact optional set_x(Option<T>) (now replace_x), endpoint.x qualification
PageLimit, paginate(...), PaginatedRequest, PageResponseRepresentation
fn accept() on response representations (now const ACCEPT)
RequestBody metadata callback, try_bind, runtime auth(...)/replace_auth(...)
path atom lists (now relative path templates), dynamic credential selection
```

## Generated Rustdoc order

For each endpoint:

1. author domain documentation/deprecation;
2. exact generated call and output;
3. required and configurable values/control methods;
4. effective request/response representation and provenance;
5. method/logical route/query/header summary with sensitivity markers;
6. accepted status and effect;
7. body replay/page capability and response lifecycle;
8. auth/challenge structure;
9. timeout/retry/rate behavior;
10. pagination and terminal capabilities;
11. stable IDs, error/cancellation/unknown-outcome guidance.

The common caller should understand the first screen without seeing bridge generics or decoder drivers.

## Source anchors

Use compile-checked example anchors for README, quick start, overview, value
matrix, representation, auth, retry, pagination, runtime endpoints, and the
Riot walkthrough. `cargo run --locked -p concord-xtask -- docs-check --locked`
is the single canonical checker used by `just docs`, local release
verification, and the GitHub Actions documentation job. Its machine-maintained
canonical-source list prevents the front-door declarations from reverting to
unchecked pseudocode. The PowerShell entry point is only a thin wrapper around
the Rust command.

## Documentation completion

Before release:

- all active examples use one syntax;
- every public item has docs under `-D warnings`;
- no old public name appears outside migration/rejected-history context;
- effective docs show representation/status/effect origins;
- links and source anchors pass;
- user-task protocol uses the same documentation a release user sees.
