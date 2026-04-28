# Macro Architecture

The v4 macro pipeline is:

```text
parse raw syntax
  -> normalize syntax-shaped scopes/endpoints
  -> resolve inheritance and names
  -> ResolvedApi / ResolvedEndpoint
  -> codegen
```

The current implementation keeps normalization inside `parse` plus `sema` rather than a separate compiled module. The invariant is still strict: codegen consumes `ResolvedApi` and `ResolvedEndpoint`, not raw AST ancestry.

## Layers

| Type | Layer | Purpose | Status |
| --- | --- | --- | --- |
| `ClientDef` | raw AST | Syntax accepted by the parser and old-syntax diagnostics | Parser only |
| `LayerDef` | normalized raw tree | Scope route/policy node after parser normalization | Parser/sema only |
| `EndpointDef` | raw leaf | Endpoint syntax before name and inheritance resolution | Parser/sema only |
| `LayerIr` | resolver state | Internal route/policy/auth state while walking scopes | Sema only |
| `ResolvedPolicySpec` | resolved model | Effective scope policy stack, endpoint policy, and auth plan inputs | Codegen input |
| `ResolvedEndpoint` | resolved model | Final route pieces, facade path, params, policy, body, response, pagination | Codegen input |
| `ResolvedApi` | resolved model | Final client-level model and endpoint collection | Codegen input |

## Ownership

`parse` may recognize removed syntax only to produce v4 replacement errors.

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

