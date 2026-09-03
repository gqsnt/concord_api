# Quick start

This walkthrough uses the compiled catalog example in
`crates/concord-examples`.

## 1. Define domain types and a stable default

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_types -->
```rust
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

## 2. Declare one contained client tree

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

The output type is exactly `Vec<ItemSummary>`; `Json<Vec<ItemSummary>>` owns
status/media/acquisition/conversion. `category?` creates one outer omission
slot. `offset` and `count` are defaulted once per request snapshot and remain
available to the pagination adapter.

## 3. Build and configure immutable snapshots

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

All controls consume a snapshot and return another. Nothing above performs HTTP
or allocates an operation lifecycle.

## 4. Execute one page or a bounded prefix

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_execution -->
```rust
pub async fn execute(client: &CatalogApi) -> Result<(usize, usize), concord::ApiError> {
    let one_page: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .with_category("hardware".to_owned())
        .await?;

    let first_two_pages: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .paginate(PageLimit::Prefix(2))
        .await?;

    Ok((one_page.len(), first_two_pages.len()))
}
```

Polling the terminal begins the managed executor. Pagination shares one
deadline and independent page/item/byte/cycle/attempt bounds.

## 5. Plan or transport intent

Use `let intent = CatalogApi::unbound().regional(region).item_summaries();` to
construct the same endpoint tree without a runtime. `intent.description()` is
static. `client.plan(&intent)` performs credential-free/redaction-aware public
shaping. `client.request(intent)` binds it for execution. None of these operations
executes until the returned request terminal is polled.

Continue with [generated client details](generated_client.md),
[authentication](auth.md), and [planning](planning_and_observation.md).
