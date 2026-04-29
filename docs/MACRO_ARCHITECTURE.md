# Macro Architecture

The v5 macro pipeline is:

```text
RawAst
  -> NormApiTree
  -> ResolvedApi / ResolvedEndpoint
  -> codegen
```

The parser owns source syntax. Semantic resolution owns inheritance, names, facade paths, policy materialization, route materialization, auth plans, and pagination plans. Codegen consumes only the resolved model.

## Layers

| Type | Layer | Purpose | Status |
| --- | --- | --- | --- |
| `RawApi` | raw AST | Source syntax accepted by the v5 parser | Parser only |
| `RawScope` | raw AST | Scope syntax before inheritance resolution | Parser/sema only |
| `RawEndpoint` | raw AST | Endpoint stanza syntax before resolution | Parser/sema only |
| `NormApiTree` | normalized tree | Syntax-shaped tree after parser normalization | Sema only |
| `ResolvedPolicySpec` | resolved model | Effective policy and auth plan inputs | Codegen input |
| `ResolvedEndpoint` | resolved model | Final route pieces, facade path, params, policy, body, response, pagination | Codegen input |
| `ResolvedApi` | resolved model | Final client-level model and endpoint collection | Codegen input |

## Ownership

`parse` accepts strict v5 syntax.

`sema` resolves:

- client/scope/endpoint variables;
- route fragments;
- inherited headers/query/timeout;
- auth requirements;
- cache/retry/rate-limit profiles;
- facade path and endpoint aliases;
- pagination controller inputs.

`codegen` emits:

- client wrapper and facade;
- auth state;
- endpoint structs;
- `Endpoint::plan` implementations;
- request plans and pagination plans.

Codegen must not walk raw AST ancestry or recompute inheritance.
