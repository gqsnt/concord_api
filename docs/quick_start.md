# Concord v2.1 quick start

- **Audience:** first-time API authors and callers
- **Status:** `2.1.0-beta.1` unpublished beta syntax
- **Normative references:** [DSL specification](spec.md), [generated client](generated_client.md)

This tutorial is one compile-checked program maintained in
`crates/concord-examples/src/getting_started/domain_tree.rs`. Every Rust and
full Concord declaration below is copied from that module.

## Add dependencies

```toml
[dependencies]
concord = { path = "../concord_api2/crates/concord", features = ["json"] }
serde = { version = "1", features = ["derive"] }
```

This is a checkout-only beta dependency. A registry line cannot be finalized
until the owner approves and controls the public package namespace; the Rust
import remains `concord`. Concord v2.1 has an MSRV of Rust 1.86.

## Imports and domain types

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_types -->
```rust
use std::error::Error;
use std::num::NonZeroU32;

use concord::pagination::PaginatedRequest;
use concord::{Json, PageLimit, api};
use serde::Deserialize;

/// Default page size retained by each endpoint snapshot.
pub const DEFAULT_PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(25) {
    Some(value) => value,
    None => unreachable!(),
};

/// One item returned by the example catalog API.
#[derive(Debug, Deserialize)]
pub struct ItemSummary {
    /// Stable item identifier.
    pub id: String,
    /// Human-readable title.
    pub title: String,
}
```

The payload types remain ordinary Rust types. Concord presence and wire
cardinality are declared separately from those types.

## Declare the complete client

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
```concord
api! {
    client CatalogApi {
        timeout 30s;
        base "http://catalog.example.test/";
        response Json;

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
            ) -> Vec<ItemSummary> {
                path ["items"];
                query {
                    "category" = endpoint.category;
                    "offset" = endpoint.offset;
                    "count" = endpoint.count;
                }
                paginate via concord::pagination::OffsetPagination {
                    offset -> endpoint.offset;
                    page_size -> endpoint.count;
                }
            }
        }
    }
}
```

`response Json;` is inherited by the endpoint. `category?` owns outer
presence, while `offset` and `count` retain declaration defaults. The
pagination controller binds only the declared endpoint-owned live values.

## Build the client

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_builder -->
```rust
pub fn build_client() -> Result<CatalogApi, concord::ClientBuildError> {
    CatalogApi::builder()
        .set_tenant("acme".to_owned())
        .build()
}
```

Building creates an immutable configuration snapshot. It performs no provider,
limiter, observer, body, request, or transport work.

## Construct and await one ordinary request

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_ordinary_request -->
```rust
pub fn ordinary_request(client: &CatalogApi) {
    let _request = client
        .regional("eu".to_owned())
        .item_summaries();
}
```

Request construction is also inert. Awaiting the same domain-shaped request
returns the exact declared output:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_await_request -->
```rust
pub async fn fetch_one_page(client: &CatalogApi) -> Result<Vec<ItemSummary>, concord::ApiError> {
    client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .await
}
```

The caller does not mention JSON because the response representation is a
stable contract default.

## Use optional and default controls

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_controls -->
```rust
pub fn controlled_request(client: &CatalogApi) {
    let _request = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .omit_category()
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .reset_count();
}
```

`set_category` accepts the exact effective `Option<String>` and
`omit_category` selects Concord outer absence. `count` has a retained default,
so `reset_count` restores that declaration value. There are no `with_*` or
`clear_*` aliases.

## Paginate explicitly

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_pagination -->
```rust
pub fn paginated_request(client: &CatalogApi) {
    let _: PaginatedRequest<_> = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
}
```

`PageLimit::Prefix(2)` makes truncation explicit. Awaiting this terminal returns
one complete aggregate vector or an error; a bare numeric pagination coercion
does not exist.

## Inspect without executing

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_planning -->
```rust
pub fn inspect(client: &CatalogApi) -> Result<(), concord::ApiError> {
    let endpoint = CatalogApi::unbound()
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()));
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _bound_plan = request.plan()?;
    Ok(())
}
```

Descriptions and plans expose bounded, credential-free structure. Planning
does not prepare a body, acquire credentials or rate admission, invoke
observers, or send HTTP.

## Next

- [Requests and values](requests_and_values.md)
- [Generated client](generated_client.md)
- [Request bodies](request_bodies.md)
- [Authentication](auth.md)
- [Pagination](pagination.md)
- [Execution policy](execution_policy.md)
