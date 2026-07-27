# Understand generated Rust

Use the generated surface as ordinary, named Rust. The declaration determines
the stable client, scope, endpoint, and request names; exact concrete
signatures then appear in generated Rustdoc and compiler diagnostics.

## Follow one declaration into names

For the guide contract, Concord emits this public shape:

| Declaration role | Generated Rust |
|---|---|
| `client ReferenceApi` | `ReferenceApi` and `ReferenceApiBuilder` |
| `scope regions` | method/module `regions` and type `Regions` |
| `METHOD("PROPFIND") fetch` | method `fetch` and endpoint type `Fetch` |
| bound endpoint | `FetchRequest` |

The scope-oriented module path is
`reference_api::regions`. Normal application code therefore reads like the
service domain:

```rust
let request: reference_api::regions::FetchRequest = client
    .regions("eu-west".to_owned())
    .fetch("42".to_owned(), vec!["stable".to_owned()]);
```

Required scope and endpoint values are arguments. Optional and defaulted
values are consuming controls on the owner that retains them.

## Distinguish the endpoint from its request

Every declared endpoint has an explicit endpoint type. It stores the public
inputs needed to identify and shape an operation, but it is not bound to a
client runtime:

```rust,ignore
{{#include ../../crates/concord-examples/src/contracts/explicit_endpoints.rs:explicit_endpoint}}
```

`Fetch::new(...)` constructs the explicit endpoint. `client.request(endpoint)`
binds it and returns `FetchRequest`. The direct scope method produces that same
request type. This symmetry is useful when code must choose a client later,
inspect a safe plan, or accept generated endpoints through Concord's sealed
generated endpoint trait.

For ordinary calls, prefer the direct method. The explicit endpoint adds a
useful value boundary; it does not provide lower-level access or different
execution behavior.

## Read capabilities from methods

A named request exposes only terminals and controls valid for that endpoint:

- awaiting it returns the declared response value;
- `.response()` returns `ApiResponse<T>` when typed response metadata is
  supported;
- `.response_bytes()` returns `ApiResponse<Bytes>` for buffered and discard
  response modes while preserving declared acceptance and `Accept`;
- `.paginate(PageLimit)` exists only for a paginated endpoint;
- request-local timeout controls exist according to the generated contract.

Invalid operations are absent from the type rather than deferred to a runtime
flag. A bodyless endpoint cannot acquire a body setter, and a non-paginated
endpoint does not have `.paginate()`. Streaming response modes reject
`.response_bytes()` at compile time through a sealed mode bound.

Request wrappers expose controls for their own non-required endpoint values.
They do not copy controls from an enclosing scope. Explicit endpoint snapshots
do retain the non-required scope values needed to represent the whole
unbound operation.

## Use generated Rustdoc for exact signatures

Generated Rustdoc is most useful when you know the declaration name and need
the exact concrete type, argument order, control, terminal, endpoint
description, or applicable declared rate limits. Open the generated client
type first, follow its scope methods, and then inspect the endpoint or request
type.

Use the compiler the same way. If a response specification, body
specification, value formatter, or optional feature is missing, its trait
diagnostic points at the declaration or call site. Concord does not infer
capability from familiar type names; the enabled type's trait implementations
decide what it can do.

No generated prelude or extension trait is required to reveal normal methods.
There are no legacy bare setters, `with_*` aliases, or mutable request
setters. Construction/navigation keep declaration names; value mutation uses
the stable `set_*`, `clear_*`, and `reset_*` vocabulary.

The [generated API reference](../reference/generated-api.md) owns the exact
naming and capability matrix. Use this guide to navigate it, not as a duplicate
signature catalogue.

## Related

- [Build and share clients](client-construction.md)
- [Requests, planning, and errors](requests-planning-errors.md)
- [Generated API reference](../reference/generated-api.md)
- [Complete DSL reference](../reference/dsl.md)
- [Feature flags](../reference/features.md)
