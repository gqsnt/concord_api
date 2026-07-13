# Internals

This page describes the implementation model for maintainers.

## Design Invariants

Maintainers should read `design_invariants.md` before changing DSL syntax, semantic resolution, code generation, or runtime pipeline behavior.

Maintain the consumer-facing safety boundary described in [Security Model](security_model.md) when changing public surfaces or documentation.

## Compiler Pipeline

```text
tokens
  -> RawAst
  -> NormApiTree
  -> ResolvedApi + FacadeIr
  -> codegen
```

Code generation consumes resolved semantic data. Parser structures stay on the parsing and normalization side of the boundary.

Profile use names are preserved as endpoint documentation metadata even though profile semantics are lowered into ordinary auth, retry, and rate-limit policy data.

Generated client code targets `concord_core::__private` for plan and plumbing internals. `concord_core::internal` remains a deprecated hidden compatibility alias during the transition, but it is not the intended user import path.

## Runtime Pipeline

```text
Endpoint::plan -> RequestPlan -> execute_plan
```

The core runtime is syntax-neutral. It executes request plans with fixed ordering for auth preparation, rate limiting, transport, response classification, retry, and decoding.

## Standard Body Foundation

`concord_core::advanced::DynBody` is the future common body substrate. It is a
`BoxBody<Bytes, BodyError>` and preserves data frames, trailer frames, frame
order, truthful `SizeHint` values, and `is_end_stream`. Empty and buffered
bodies use the upstream `http-body-util` primitives.

`BodyError` is the typed body error authority. Its `Debug`, `Display`, and
source chain expose only safe categories and bounded limit metadata; producer
messages and body bytes are never retained. Send-only inputs use a safe
exclusive-poll adapter that holds a standard-library mutex only during the
synchronous poll operation. It does not use `unsafe`, a forwarding task, or a
buffering queue, and body construction is lazy.

Request planning keeps a single logical recipe rather than a prebuilt
`DynBody`: reusable bytes, one-shot byte streams, advanced HTTP bodies,
terminal factories, and multipart recipes remain distinguishable until an
attempt is materialized. The current conversion to `DynBody` is a private
native request materialization path and has no public executor boundary
point. Exact stream lengths are guards, not `SizeHint` claims; they reject
underflow and overflow without retaining payload diagnostics.

`LimitedBody` is the reusable frame-aware limiter. It counts bytes in data
frames, leaves trailers uncounted and unchanged, and becomes terminal after a
typed over-limit error. Native streaming responses use direct data chunks for
normal consumption; the explicit `StreamResponse::into_body()` façade uses a
narrow private frame-aware wrapper with the same data-only byte accounting.

## Test Artifacts

Runtime behavior is covered by focused `concord_core` tests and end-to-end generated-client tests in `concord_examples/tests/`.
Parser, sema, and codegen behavior are covered by structural assertions in the owner-layer test modules rather than broad snapshot files.
