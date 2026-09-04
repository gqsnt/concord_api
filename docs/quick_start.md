# Concord v2.1 quick start

- **Audience:** first-time API authors and callers
- **Status:** target syntax; release freeze follows the evidence gates
- **Normative references:** [DSL specification](spec.md), [generated client](generated_client.md)

## Add dependencies

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord", features = ["json"] }
serde = { version = "1", features = ["derive"] }
```

Concord v2.1 keeps the workspace MSRV at Rust 1.86 unless the representation prototype proves that impossible and the change is recorded explicitly.

## Domain types

```rust
use concord::{api, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct CreateItem {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub id: String,
    pub title: String,
}

pub type ItemId = String;
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

```rust
let client = CatalogApi::builder()
    .set_tenant("acme".to_owned())
    .build()?;
```

Construction is inert. No provider, limiter, body encoder, observer, request, or HTTP work begins.

## Call an endpoint

```rust
let item: Item = client.item(item_id).await?;
```

The caller does not mention JSON because representation is a stable contract default.

## Send a body

```rust
let created: Item = client.create(CreateItem {
    title: "System Design".to_owned(),
}).await?;
```

The body parameter type is exactly `CreateItem`. The result type is exactly `Item`.

## Inspect without executing

```rust
let request = client.item(item_id);
let description = request.description();
let plan = request.plan()?;
let item = request.await?;
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

```rust
let request = client
    .items()
    .set_category(category_from_config) // Option<String>
    .set_category(Some("books".to_owned()))
    .omit_category();
```

- `set_category` accepts the exact effective `Option<String>`.
- `omit_category` selects outer absence.

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
