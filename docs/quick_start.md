# Concord v2.1 quick start

- **Audience:** first-time API authors and callers
- **Status:** `2.1.0-beta.1` integration-candidate syntax; release freeze follows the evidence gates
- **Normative references:** [DSL specification](spec.md), [generated client](generated_client.md)

## Add dependencies

```toml
[dependencies]
concord = { path = "../concord_api2/crates/concord", features = ["json"] }
serde = { version = "1", features = ["derive"] }
```

This path form is the truthful checkout-only beta example. A registry install
line cannot be finalized until the owner approves and secures the public
package namespace in Gate 0; the Rust import remains `concord`.

Concord v2.1 keeps the workspace MSRV at Rust 1.86.

## Domain types

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

`RequestAtom` is a stable public-protocol role. Concord does not silently use a human-facing `Display` implementation.

## Declare one client

```concord-pseudocode
api! {
    client CatalogApi {
        base "https://catalog.example.test/";
        timeout 5s;
        request Json;
        response Json;

        values {
            #[sensitive]
            tenant: String;
        }

        path ["v1"];
        query { "tenant" = client.tenant; }

        GET item(id: ItemId) -> Item {
            id "catalog.item.get";
            path ["items", endpoint.id];
        }

        POST create(
            body input: CreateItem,
        ) -> Item {
            id "catalog.item.create";
            path ["items"];
            status 201;
            effect non_idempotent;
        }
    }
}
```

Read the declaration in layers:

1. The client owns the fixed origin, finite timeout, and common JSON representations.
2. The client value owns the tenant protocol value and sensitivity.
3. Each endpoint line contains exact caller input and exact awaited output.
4. Endpoint blocks contain only local route, status, effect, and exceptional capabilities.

## Build the client

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_usage -->
```rust
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let client = CatalogApi::builder()
        .set_tenant("acme".to_owned())
        .build()?;

    let _: PaginatedRequest<_> = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
    Ok(())
}
```

Construction is inert. No provider, limiter, body encoder, observer, request, or HTTP work begins.

## Call an endpoint

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_execution -->
```rust
pub async fn execute(client: &CatalogApi) -> Result<(usize, usize), concord::ApiError> {
    let one_page: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .await?;

    let first_two_pages: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .paginate(PageLimit::Prefix(2))
        .await?;

    Ok((one_page.len(), first_two_pages.len()))
}
```

The caller does not mention JSON because representation is a stable contract default.

## Send a body

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#buffered_requests -->
```rust
    let _: body_api::JsonRoundTripRequest = client.json_round_trip(vec![1, 2, 3]);
    let _: body_api::TextRoundTripRequest = client.text_round_trip("bounded text".to_owned());
    let _: body_api::CustomRoundTripRequest = client.custom_round_trip("custom payload".to_owned());
```

The body parameter type remains the exact declared Rust type (`Vec<u32>`,
`String`, or the custom codec's `String` here); there is no generated payload
wrapper.

## Inspect without executing

<!-- concord-source: crates/concord-examples/src/contracts/catalog_v2.rs#catalog_planning -->
```rust
    let _plan = request.plan()?;
    let _pagination = request.paginate(PageLimit::CompleteWithin(
        NonZeroU32::new(20).expect("positive"),
    ));
    let _accepted: concord::endpoint::AcceptedBytesRequest<_> =
        regional.item("item-42".to_owned()).accepted_bytes();

    let endpoint = CatalogApi::unbound()
        .regional("eu-west".to_owned())
        .item("item-42".to_owned());
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _response: concord::endpoint::ResponseRequest<_> = request.response();
    let _metadata_shape: Option<ApiResponse<Bytes>> = None;
```

Planning performs bounded public shaping but does not invoke body preparation, credentials, rate admission, retry, observers, or HTTP. Sensitive values remain structurally redacted in automatic formatting.

## Add an optional query value

```concord-pseudocode
GET items(category?: String) -> Vec<Item> {
    id "catalog.items.list";
    path ["items"];
    query { "category" = endpoint.category; }
}
```

Generated usage:

<!-- concord-source: crates/concord-examples/src/contracts/full_dsl.rs#request_controls -->
```rust
    let _: reference_api::StatusRequest = client
        .status()
        .set_verbose(Some(true))
        .omit_verbose()
        .reset_verbose()
        .set_tags(Some(vec!["stable", "documented"]))
        .omit_tags()
        .reset_tags();
```

- `set_verbose` accepts the exact effective `Option<bool>`.
- `omit_verbose` selects outer absence; `reset_verbose` restores its declaration default.

## Repeated values are explicit

```concord-pseudocode
GET search(tags: Vec<String>) -> Vec<Item> {
    id "catalog.items.search";
    path ["search"];
    query { append "tag" = each endpoint.tags; }
}
```

`Vec<String>` remains an exact Rust collection. `each`—not the type name—owns repeated wire occurrences.

## Next

- [Requests and values](requests_and_values.md)
- [Generated client](generated_client.md)
- [Execution policy](execution_policy.md)
- [Modeling APIs](modeling_apis.md)
