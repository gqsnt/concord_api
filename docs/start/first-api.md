# Your first API

Declare one client and one endpoint with `concord::api!`. This complete
example lives in `concord-examples` and is compiled with the workspace:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/getting_started/minimal.rs:first_api}}
```

The declaration has four important parts:

- `client MinimalApi` chooses the generated client name.
- `base` supplies the fixed URL shared by its endpoints.
- `GET health()` names a bodyless endpoint and its HTTP method.
- `-> NoContent` says an accepted response has no application body value.

The endpoint-local `path ["health"]` appends one encoded segment to the base
path. The resulting logical URL ends in `/v1/health`. The `.test` top-level
domain is reserved for examples; this declaration cannot accidentally reach a
real public service.

## Generated names

At a high level, the compiler emits:

```text
MinimalApi
minimal_api::HealthRequest
```

Because this declaration has no required client inputs, `MinimalApi::new()` is
the shortest construction entry. A direct call to `client.health()` produces
`HealthRequest`.

Exact signatures are visible in generated Rustdoc. The
[generated API reference](../reference/generated-api.md) owns the stable
naming and capability matrix, including builders and reusable endpoint values;
this page focuses on the first working path.

## What the compiler checks

The declaration compiler owns structural facts such as:

- exactly one client per macro invocation;
- a valid absolute HTTP or HTTPS base with a host;
- `snake_case` scope and endpoint source names;
- valid path, query, header, authentication, pagination, and rate-limit
  structure;
- a coherent set of generated names and capabilities.

Rust then checks that `NoContent` implements the required response
specification trait. Other response types work the same way: `Bytes` and
`Text` are core representations, while `Json<T>` is available with the
`json` feature.

No network connection is opened while the macro expands or while an endpoint
value is constructed. The declaration defines a contract; execution remains a
separate async operation.

## Adapt the declaration

For a real service:

1. replace the example base with the service's stable base URL;
2. name the endpoint after the application task;
3. add required endpoint values as ordinary parameters;
4. shape the path, query, and public headers from those values;
5. select a response specification supported by an enabled façade feature.

For example, an endpoint parameter `id: u64` becomes a required method
argument, and `path ["items", ep.id]` formats it into its own encoded path
segment. Keep secrets out of public shaping; authentication has a separate
private declaration path.

The complete grammar is intentionally not repeated here. Use the
[DSL reference](../reference/dsl.md) when you need exact clause ordering,
optional/default syntax, inheritance, or diagnostics.

## Related

- [Make the first request](first-request.md)
- [Understand generated Rust](../guide/generated-rust.md)
- [Complete DSL reference](../reference/dsl.md)
- [Generated API reference](../reference/generated-api.md)
