# Concord

Concord v2 describes one typed HTTP API contract and projects it into a
domain-shaped Rust client, bounded managed execution, and credential-free,
redaction-aware operational evidence.

The declaration owns domain values, lexical scopes, wire placement,
authentication, response semantics, pagination, and quota identity. Transport,
retry policy, limiter implementation, gates, observation, and finite runtime
limits remain cohesive client configuration.

## One declaration, one domain tree

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
```rust
api! {
    client CatalogApi {
        timeout off;
        base "http://catalog.example.test/";
        values {
            tenant: String;
        }
        path ["api"];
        query { "tenant" = client.tenant; }


    scope regional(region: String) {
        subdomain replace [regional.region, "api"];
        path ["v1", "catalog"];

        GET item_summaries(
            category?: String,
            offset: u64 = 0,
            count: std::num::NonZeroU32 = DEFAULT_PAGE_SIZE,
        ) -> Vec<ItemSummary> using Json<Vec<ItemSummary>> {
            path ["items"];
            query {
                "category" = endpoint.category;
                "offset" = endpoint.offset;
                "count" = endpoint.count;
            }
            paginate with concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}}
```

The generated use follows the same lexical tree. Required non-defaulted scope
and endpoint values are positional; optional/defaulted values are exact typed
controls; terminals exist only for declared capabilities.

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_usage -->
```rust
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let client = CatalogApi::builder()
        .set_tenant("acme".to_owned())
        .build()?;

    let _: PaginatedRequest<_> = client
        .regional("eu".to_owned())
        .item_summaries()
        .with_category("hardware".to_owned())
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
    Ok(())
}
```

`CatalogApi::unbound()` provides the same tree without a runtime. An intent can
be stored, described, planned against a client, and later bound. Construction
and planning are inert; managed effects start only when a terminal is polled.

## Core guarantees

- Rust type spelling never controls optionality or repetition. `?` adds one
  outer presence layer and `each` selects repeated placement.
- Public shaping and credential-free observation happen before private
  authentication placement.
- One executor owns direct, unbound-bound, runtime-defined, retry, recovery,
  pagination, response, and accepted-byte paths.
- Every growing dimension is bounded, and pagination shares one aggregate
  deadline across all pages and attempts.
- Plans and ordinary evidence structurally exclude credentials and propagate
  `#[redact]`; visible public data may still be sensitive.
- Rate keys use canonical typed framing, and a rate-limited contract cannot
  build without an explicit limiter.

## Use from this checkout

```toml
[dependencies]
concord = { path = "../concord_api2/crates/concord", features = ["json"] }
```

The workspace uses Rust edition 2024 and MSRV 1.86. Packages remain private
(`publish = false`).

Start with the [documentation index](docs/README.md), the
[quick start](docs/quick_start.md), or the normative
[DSL specification](docs/spec.md). Existing v1 contracts should follow the
[migration guide](docs/migration_v1_to_v2.md).

The complete repository gate is:

```text
just verify
```

See the [v2 constitution](FINAL.md) and
[maintainer internals](dev_doc/internals.md) for gate ownership.
