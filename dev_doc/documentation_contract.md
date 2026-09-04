# Documentation contract

## Truth rules

1. Documentation describes implemented behavior in the branch where it lives.
2. Selected target behavior not yet implemented is marked explicitly and should remain in the implementation kit or feature branch, not active release docs.
3. `docs/design_invariants.md`, `docs/spec.md`, and `docs/execution_policy.md` are the highest normative in-repository documents.
4. Generated Rustdoc, static descriptions, semantic diff, and hand-written docs consume the same vocabulary and semantic graph.
5. A public example is compile checked or clearly labeled conceptual pseudocode.
6. A claim that depends on Rust coherence, performance, user cognition, or operation remains provisional until its gate passes.
7. No doc calls a plan “safe,” a local limiter “distributed,” a timeout “operation-wide,” or a response “complete” unless the exact guarantee is implemented.
8. Secrets, body values, sensitive public values, and raw rate keys never appear in example diagnostic output.
9. `dev_doc/release_gate_status.md` is the only lifecycle/gate owner. Historical
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

Use compile-checked example anchors for quick start, value matrix,
representation, auth, retry, pagination, runtime endpoint, and Riot
walkthrough. `cargo run --locked -p concord-xtask -- docs-check --locked` is the
single canonical checker used by `just docs`, local release verification, and
the GitHub Actions documentation job. The PowerShell entry point is only a thin
compatibility wrapper around that Rust command.

## Documentation completion

Before release:

- all active examples use one syntax;
- every public item has docs under `-D warnings`;
- no old public name appears outside migration/rejected-history context;
- effective docs show representation/status/effect origins;
- links and source anchors pass;
- user-task protocol uses the same documentation a release user sees.
