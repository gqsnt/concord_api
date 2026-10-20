# Concord v3 quick start

- **Audience:** first-time API authors and callers
- **Status:** `3.0.0-beta.1` unpublished beta
- **Normative references:** [DSL specification](spec.md), [generated client](generated_client.md)

This tutorial is compile-checked: every Rust block and full declaration below
is an anchored excerpt of `crates/concord-examples/src/getting_started/`. The
hosts are fixtures (`*.example.test`), not live services. Blocks marked
*construction-only* build values without executing them.

## Add dependencies

```toml
[dependencies]
concord = { path = "path/to/concord/crates/concord", features = ["json"] }
serde = { version = "1", features = ["derive"] }
```

This is a checkout-only beta dependency: publication is disabled and a registry
line cannot be finalized until the owner approves the public package
namespace. The Rust import is `concord`. The MSRV is Rust 1.86.

## 1. Declare a client

The smallest useful client declares a base URL and one endpoint:

<!-- concord-source: crates/concord-examples/src/getting_started/minimal.rs#first_api -->
```rust
use concord::{Empty, api};

api! {
    client MinimalApi {
        base "http://minimal.example.test/v1/";

        GET health() -> () via Empty {
            path "health";
        }
    }
}
```

- `client MinimalApi` generates the `MinimalApi` type and a `minimal_api`
  support module holding its builder and named requests.
- The base owns the fixed origin and base path. `path "health"` appends one
  relative segment; paths never start or end with `/`.
- The omitted `timeout` selects the visible v3 default of 30 seconds.
- `-> () via Empty` names the output type and its representation explicitly:
  nothing is guessed from `()`. `Empty` accepts only an empty accepted body.

## 2. Build it

<!-- concord-source: crates/concord-examples/src/getting_started/minimal.rs#build_client -->
```rust
/// Builds the minimal client with Concord's default runtime components.
///
/// `new()` exists because nothing is required: no root value, secret,
/// provider or limiter choice remains.
pub fn build_client() -> Result<MinimalApi, concord::ClientBuildError> {
    MinimalApi::new()
}
```

Building performs no network access. A client with required values, secrets or
a rate policy has no `new()`; use `MinimalApi::builder()`, whose `build()`
reports every missing input together in one `ClientBuildError`.

## 3. Send a request

<!-- concord-source: crates/concord-examples/src/getting_started/minimal.rs#first_request -->
```rust
/// Executes one health request and accepts a declared empty response.
pub async fn execute(client: &MinimalApi) -> Result<(), concord::ApiError> {
    let request = client.health();
    request.await
}
```

`client.health()` returns an owned `minimal_api::HealthRequest`. Holding it
does not contact the server. `.await` executes exactly one managed operation:
it validates the response status (200-299 by default), applies the declared
representation and returns the typed output or a sanitized `ApiError`.

The maintained test runs this exact function against a local scripted server
through an explicit proxy (a test fixture, not a production pattern):

<!-- concord-source: crates/concord-examples/tests/local_runtime.rs#first_request_loopback -->
```rust
#[tokio::test]
async fn getting_started_request_executes_against_loopback() {
    use concord::http::SafeProxy;
    use concord_examples::getting_started::minimal::{MinimalApi, execute};
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    let server =
        ScriptedServer::start([ResponseScript::status(204, Vec::new())]).expect("health server");
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let client = MinimalApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("minimal client");

    execute(&client).await.expect("successful health request");

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "GET");
    assert!(requests[0].target().ends_with("/v1/health"));
}
```

A proxy changes connection routing only; the logical origin stays
`minimal.example.test`.

## 4. A domain-shaped client

Real APIs have tenants, regions and lists. Support types live in ordinary Rust:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_types -->
```rust
use std::error::Error;
use std::num::NonZeroU32;

use concord::pagination::{OffsetInit, OffsetPagination, Prefix};
use concord::{Json, api};
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

The declaration is the caller tree:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
<!-- concord-dsl -->
```rust
api! {
    client CatalogApi {
        base "http://catalog.example.test/";
        timeout 30s;
        response Json;

        values {
            query tenant: String;
        }

        path "api";

        scope regional(region: String) {
            subdomain replace [region, "api"];
            path "v1/catalog";

            GET item_summaries(
                query category?: String,
                query offset: u64 = 0,
                query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
            ) -> Vec<ItemSummary> {
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }
        }
    }
}
```

- `values { query tenant: String; }` declares a required client value and
  places it directly as the `tenant` query key of every request.
- `scope regional(region: String)` is a navigation owner. `subdomain replace
  [region, "api"]` makes the host `{region}.api.catalog.example.test`; `region`
  is a bare local name because the clause belongs to the scope.
- `query category?: String` is optional and omitted when absent;
  `query offset: u64 = 0` and `query count: ... = DEFAULT_PAGE_SIZE` are
  initialized.
- `response Json` is the client's response default. `paginate via` declares the
  traversal controller, its public `init` value and the endpoint slot its
  cursor drives.

The client value is supplied once, to the builder:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_builder -->
```rust
pub fn build_client() -> Result<CatalogApi, concord::ClientBuildError> {
    CatalogApi::builder().set_tenant("acme").build()
}
```

`set_tenant` accepts `impl IntoValue<String>`: a `String`, `&str` or `&String`,
nothing else. Awaiting the list returns **one** decoded page:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_await_request -->
```rust
pub async fn fetch_one_page(client: &CatalogApi) -> Result<Vec<ItemSummary>, concord::ApiError> {
    client
        .regional("eu")
        .item_summaries()
        .set_category("hardware")
        .await
}
```

`client.regional("eu")` returns an owned `catalog_api::regional::Regional`
handle; it does not borrow the client expression.

## 5. Choose inputs exactly

*Construction-only.* Each value has the controls its declared shape allows:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_controls -->
```rust
pub fn controlled_request(client: &CatalogApi) {
    let _request = client
        .regional("eu")
        .item_summaries()
        .set_category("hardware")
        .omit_category()
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .reset_count();
}
```

| Declared shape | Controls |
|---|---|
| required, uninitialized (`region: String`) | positional constructor argument only |
| initialized (`offset: u64 = 0`) | `set_offset`, `set_offset_if_some`, `reset_offset` |
| optional (`category?: String`) | `set_category`, `replace_category`, `set_category_if_some`, `omit_category` |
| optional, initialized (`x?: T = V`) | all of the above plus `reset_x` |

`replace_x(Option<T>)` replaces the slot exactly (`None` is absence);
`set_x_if_some(None)` means "no new opinion" and keeps the current selection.
See [Generated client](generated_client.md) for the complete matrix.

## 6. More pages are an explicit terminal

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_await_pagination -->
```rust
pub async fn fetch_two_pages(
    client: &CatalogApi,
) -> Result<Prefix<ItemSummary>, concord::ApiError> {
    client
        .regional("eu")
        .item_summaries()
        .collect_prefix(2)
        .await
}
```

`collect_prefix(n)` returns a `Prefix<Item>` whose summary says whether the
provider proved exhaustion or the page bound stopped the traversal.
`collect_all(NonZeroU32)` returns a `Vec` only after the provider proved
exhaustion, and `pages(n)` gives lazy page-by-page control. See
[Pagination](pagination.md).

## 7. Inspect without sending

*Construction-only.* Unbound intents, descriptions and plans perform no I/O:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_planning -->
```rust
pub fn inspect(client: &CatalogApi) -> Result<(), concord::ApiError> {
    let endpoint = CatalogApi::unbound()
        .regional("eu")
        .item_summaries()
        .set_category("hardware");
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _bound_plan = request.plan()?;
    Ok(())
}
```

A plan renders the sanitized URL and public headers and the effective
controls; it never prepares a body, acquires credentials or reserves quota. See
[Planning and observation](planning_and_observation.md).

## 8. Two more declaration shapes

A root value can be a typed DNS label:

<!-- concord-dsl -->
```rust
client RegionalApi {
    base "https://example.test/";
    values {
        region: String;
    }
    subdomain [region, "api"];

    GET health() -> serde_json::Value via Json {
        path "health";
    }
}
```

Credentials are declared once as sources and placed by `auth` blocks. The
builder then requires the secret (`.token(SecretString)`), and an `https` base
needs the default `default-tls` feature:

<!-- concord-dsl -->
```rust
client PrivateApi {
    base "https://private.example.test/";
    credentials {
        secret token;
        credential bearer_token = bearer(secret.token);
    }
    auth { bearer bearer_token; }

    GET account() -> serde_json::Value via Json;
}
```

## Next steps

- [Modeling APIs](modeling_apis.md) walks through the canonical Catalog with
  credentials, quotas, absence, accepted bytes, envelope pages and a stream.
- [Generated client](generated_client.md) lists every generated name.
- [DSL specification](spec.md) is the normative grammar.
