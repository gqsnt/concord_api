# 2. Understand generated Rust

The declaration compiler emits ordinary, scope-oriented Rust. Given:

```rust
api! {
    client CatalogApi {
        base "https://catalog.example.test/";
    }

    scope products(region: String) {
        GET by_id(id: u64, verbose: Option<bool>) -> Bytes;
    }
}
```

the stable public shape is:

```text
CatalogApi
CatalogApiBuilder
catalog_api::products::Products
catalog_api::products::ById
catalog_api::products::ByIdRequest

client.products(region).by_id(id)
client.request(catalog_api::products::ById::new(region, id))
```

Required values are ordered constructor or method arguments. Optional and
defaulted values use explicit inherent controls on the scope, endpoint, or
request value:

```text
construct or navigate  -> declaration-domain method name
mutate a value         -> set_name(value)
select absence         -> clear_name()
restore a DSL default  -> reset_name()
```

Required client-builder values also use `set_name(value)` because they mutate
builder state; required scope and endpoint values remain navigation or
constructor arguments. A named request exposes only the non-required endpoint
controls it owns, not inherited scope controls. No generated prelude,
extension trait, legacy bare setter, `with_*` alias, or mutable setter is
required or emitted for normal use.

Every endpoint supplies:

- one direct client/scope method;
- one public explicit endpoint value;
- one named request wrapper;
- one safe `EndpointDescription`;
- only the inherent capabilities valid for that endpoint.

For example, a paginated request has `.paginate(...)`; a non-paginated request
does not. Response metadata and dangerous terminals similarly appear only
when valid. The sealed generated endpoint trait supports generic composition
without exposing implementation details; prefer the concrete generated names
for application code.

Direct and explicit forms bind the same endpoint model and enter the same
executor. Endpoint values are reusable or cloneable only when their stored
fields permit it. Builder `reset_*` defers DSL-default evaluation to
`build()`; scope/endpoint/request `reset_*` restores the retained evaluated
default without re-evaluation. See the exact
[generated API reference](../reference/generated-api.md).

## Running proof

The
[explicit endpoint module](../../crates/concord-examples/src/contracts/explicit_endpoints.rs)
uses both forms. The
[full contract](../../crates/concord-examples/src/contracts/full_dsl.rs)
shows deterministic scope/type names, while the
[construction test](../../crates/concord-examples/tests/construction.rs)
type-checks the public surface.

Next: [Build a client and make requests](03-build-a-client-and-make-requests.md).
