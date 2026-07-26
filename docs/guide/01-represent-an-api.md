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

## Value namespaces

Request shaping uses four explicit namespaces:

```text
vars.locale      client value
scope.region     visible enclosing-scope value
ep.filename      current endpoint value
secret.api_key   protected client secret
```

`vars`, visible `scope` values, and `ep` values may be used only where their
lexical owner exists. A scope clause cannot refer to an endpoint value because
the endpoint does not exist there. A `scope.name` reference must identify one
visible scope input unambiguously; declarations with ambiguous visible names
are rejected instead of choosing one. Optional values omit the shaping
operation when absent, while defaults are evaluated at construction and can be
restored with generated `reset_*` controls. Value overrides use `set_*`, and
absence-capable values select explicit absence with `clear_*`. A value may
both accept absence and have a default, in which case it exposes both helpers.

Use `fmt[...]` to concatenate public atoms into one host label, path segment,
query value, or header value:

```rust
path [fmt[scope.region, "-", ep.filename, ".json"]];
```

Only public `vars`, `scope`, and `ep` references can shape the public request.
`secret` references are accepted only by credential definitions and never
enter routes, public headers, descriptions, or credential-sanitized URLs.
These restrictions make shaping deterministic and keep authentication
material inside the private runtime.

Fixed bases may use DNS-domain, IPv4, or IPv6 hosts. `host [...]` shaping adds
DNS labels and therefore requires a DNS-domain fixed base; the compiler
rejects an inherited or local non-empty host shape over an IP literal. An IP
base remains valid when host shaping is absent or cleared with `host off`.

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
