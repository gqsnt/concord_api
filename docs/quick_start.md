# Quick start

- **Audience:** first-time API authors and callers
- **Purpose:** tutorial
- **Normative references:** [DSL specification](spec.md), [generated client](generated_client.md)

Concord requires Rust 1.86 or newer. This tutorial uses the `json` feature.

## Add the dependency

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord", features = ["json"] }
serde = { version = "1", features = ["derive"] }
```

## Declare the supporting Rust items

Concord treats Rust types as opaque. The declaration below names one response
type and one default constant, so declare them in the same module first.

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

## Declare one domain-shaped client

The contract below has one required client value, one meaningful regional
scope, and one endpoint with an optional filter, declaration defaults, a typed
response, and explicit pagination bindings. Type and default paths resolve in
the module that contains the declaration.

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
```concord
api! {
    client CatalogApi {
        base "http://catalog.example.test/";
        vars {
            tenant: String;
        }
        path ["api"];
        query { "tenant" = vars.tenant; }
    }

    scope regional(region: String) {
        host [scope.region, "api"];
        path ["v1", "catalog"];

        GET item_summaries(
            category: Option<String>,
            offset: u64 = 0,
            count: std::num::NonZeroU32 = DEFAULT_PAGE_SIZE,
        ) -> Json<Vec<ItemSummary>> {
            path ["items"];
            query {
                "category" = ep.category;
                "offset" = ep.offset;
                "count" = ep.count;
            }
            paginate concord::pagination::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.count,
            }
        }
    }
}
```

Read it in two passes:

1. `GET item_summaries(...) -> Json<Vec<ItemSummary>>` states the domain operation.
2. `path`, `query`, and `paginate` state the local HTTP and execution mapping.

The parent nodes already state the tenant query, regional host, and shared path.
The endpoint does not repeat them.

## Understand the generated tree

The declaration generates:

```text
CatalogApi / CatalogApiBuilder
└── regional(region) -> Regional snapshot
    └── item_summaries() -> ItemSummariesRequest
```

`tenant` is required at client build time. `region` is required when selecting
the scope. `category` is absence-capable and therefore gets `set_category` and
`clear_category`. `offset` and `count` are defaulted and therefore get
`set_*` and `reset_*` controls.

## Build and construct a request

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_usage -->
```rust
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let client = CatalogApi::builder()
        .set_tenant("acme".to_owned())
        .build()?;

    let _: PaginatedRequest<_> = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category("hardware".to_owned())
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .paginate(PageLimit::Take(2));
    Ok(())
}
```

Construction is inert. No controller, limiter, credential provider, body
factory, hook, native request, or HTTP operation starts until the returned
future is polled.

## One page versus pagination

Directly awaiting `item_summaries()` performs one page using the selected
initial values. Calling `.paginate(PageLimit::Take(2))` returns a successful
finite prefix after at most two pages. A hard cap is available when natural
completion is required within a finite number of pages; see
[Pagination](pagination.md).

Both terminals produce the declared output type, so the page loop is invisible
at the call site:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_execution -->
```rust
pub async fn execute(client: &CatalogApi) -> Result<(usize, usize), concord::ApiError> {
    let one_page: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category("hardware".to_owned())
        .await?;

    let first_two_pages: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .paginate(PageLimit::Take(2))
        .await?;

    Ok((one_page.len(), first_two_pages.len()))
}
```

The example host is not a live service, so this function is compile-checked
rather than executed.

## What Concord handled

The author did not implement:

- host and path propagation;
- query-key omission for `None`;
- percent encoding;
- client, scope, endpoint, or request structs;
- default retention and reset controls;
- the page loop, cycle detection, or item limits;
- response buffering and JSON conversion;
- cancellation or completion behavior.

Those semantics are implemented once and tested centrally.

## Add authentication next

Authentication is deliberately separate from ordinary public values. Secrets
produce named builder inputs, credential sources own material acquisition, and
`auth` attachments own private placement. Continue with
[Authentication](auth.md).

## Continue

- [Mental model](mental_model.md)
- [Modeling APIs](modeling_apis.md)
- [Generated client](generated_client.md)
- [Requests and values](requests_and_values.md)
