# Internals

This page describes the implementation model for maintainers.

## Design Invariants

Maintainers should read `design_invariants.md` before changing DSL syntax, semantic resolution, code generation, or runtime pipeline behavior.

## Compiler Pipeline

```text
tokens
  -> RawAst
  -> NormApiTree
  -> ResolvedApi + FacadeIr
  -> codegen
```

Code generation consumes resolved semantic data. Parser structures stay on the parsing and normalization side of the boundary.

Behavior use names are preserved as endpoint documentation metadata even though behavior semantics are lowered into ordinary auth, retry, and rate-limit policy data.

## Runtime Pipeline

```text
Endpoint::plan -> RequestPlan -> execute_plan
```

The core runtime is syntax-neutral. It executes request plans with fixed ordering for auth preparation, rate limiting, transport, response classification, retry, and decoding.

## Test Artifacts

Runtime behavior is covered by focused `concord_core` tests and end-to-end generated-client tests in `concord_examples/tests/`.
Parser, sema, and codegen behavior are covered by structural assertions in the owner-layer test modules rather than broad snapshot files.
