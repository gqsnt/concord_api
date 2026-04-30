# Internals

This page describes the implementation model for maintainers.

## Compiler Pipeline

```text
tokens
  -> RawAst
  -> NormApiTree
  -> ResolvedApi + FacadeIr
  -> codegen
```

Code generation consumes resolved semantic data. Parser structures stay on the parsing and normalization side of the boundary.

## Runtime Pipeline

```text
Endpoint::plan -> RequestPlan -> execute_plan
```

The core runtime is syntax-neutral. It executes request plans with fixed ordering for auth, cache, inflight coordination, rate limiting, transport, response classification, retry, cache fallback, and decoding.

## Test Artifacts

Stage and generated-output snapshots live under `concord_macros/tests/snapshots/`.

Runtime behavior is covered by focused `concord_core` tests and end-to-end generated-client tests in `concord_examples/tests/`.
