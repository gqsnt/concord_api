# 1. Represent an API

A Concord declaration describes the stable remote contract: client base URL,
request values, URL shaping, authentication placement, response
representation, and finite protocol capabilities.

```rust
use concord::api;

api! {
    client CatalogApi {
        base "https://catalog.example.test/v1/";
    }

    scope products(region: String) {
        path ["regions", scope.region, "products"];

        GET by_id(id: u64) -> Bytes {
            path [ep.id];
        }
    }
}
```

Client, scope, and endpoint clauses are named and order-independent. Ordering
inside paths, query entries, headers, authentication placements, and
rate-limit windows remains meaningful. Source scope and endpoint identifiers
are `snake_case`; those identifiers directly determine method and module
names.

Use ordinary Rust types for values and representations. The compiler does not
recognize helper type names. Optionality is syntactic `Option<T>` (or
`std::option::Option<T>`), defaults use `name: Type = expression`, and a
request body is explicitly marked:

```rust
POST replace(
    id: u64,
    #[body] document: Bytes,
) -> NoContent {
    path [ep.id];
}
```

The exact accepted forms live in the [DSL reference](../reference/dsl.md).
Declare runtime policy—hooks, a concrete limiter, transport settings, and
operational observers—on the generated client builder, not in the remote
contract.

## Running proof

The importable
[minimal declaration](../../crates/concord-examples/src/getting_started/minimal.rs)
shows the smallest contract. The
[full declaration](../../crates/concord-examples/src/contracts/full_dsl.rs)
exercises the complete grammar, and the
[construction test](../../crates/concord-examples/tests/construction.rs)
imports those modules instead of recreating them.

Next: [Understand generated Rust](02-understand-generated-rust.md).
