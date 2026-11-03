# Examples (historical: V4 alpha syntax)

> **Historical.** This page records the V4 alpha (`8f7adc2`) syntax — `Json<T>` representations
> and positional root constructors. For the current surface see `README.md`, `docs/` and the
> maintained, compiled crates `examples/shapes` and `examples/quickstart`.

At the alpha, compiled and wire-tested in `examples/shapes` (`cargo test -p concord-example-shapes`). The
hand-written reqwest equivalents are in `benches/baselines`.

## S1 — simple

```rust
concord::api! {
    pub client SimpleApi {
        base "https://api.example.com/v1";
        GET health() -> Json<Health> { path "health"; }
        GET item(id: String) -> Json<Item> { path "items/{id}"; }
    }
}

let api = SimpleApi::new(reqwest::Client::new());
let item = api.item("a/b").send().await?;          // GET /v1/items/a%2Fb
```

## S2 — regional subdomain, nested scopes, shared and overridable headers

```rust
concord::api! {
    pub client CatalogApi(
        #[sensitive] header("x-api-key") api_key: String,
        header("x-api-version") api_version: String = DEFAULT_API_VERSION,
    ) {
        base "https://api.example.com/v2";
        headers { "accept" = "application/json"; }

        scope regional(region: Region) {
            subdomain "{region}";
            GET status() -> Json<Status> { path "status"; }

            scope org(org: String) {
                path "orgs/{org}/catalog";
                GET product(id: String, version?: String) -> Json<Product> {
                    path "products/{id}";
                    headers { override "x-api-version" = version; }   // per-call override
                }
            }
        }
    }
}

let api = CatalogApi::new(http, key);
let eu = api.regional(Region::Eu);                                     // https://eu.api.example.com
let p = eu.org("acme").product("p-1").send().await?;                   // x-api-version: 2024-06-01
let p = eu.org("acme").product("p-1").set_version("2025-01-01").send().await?;
let pinned = api.clone().set_api_version("2023-01-01");                // client-wide change
```

## S3 — many optional query parameters, JSON POST, DELETE

```rust
concord::api! {
    pub client ShopApi(#[sensitive] token: String) {
        base "https://api.example.com/v1";
        headers { "authorization" = "Bearer {token}"; }

        GET search(
            query q?: String,
            query("tag") each tags?: Vec<String>,
            query category?: String,
            query min_price?: u32,
            query max_price?: u32,
            query in_stock?: bool,
            query sort?: Sort,
            query cursor?: String,
            query limit: u32 = 20,
        ) -> Json<SearchPage> { path "items/search"; }

        POST create_item(body item: Json<NewItem>) -> Json<Item> { path "items"; }
        DELETE delete_item(id: String) { path "items/{id}"; }
    }
}

let page = api.search()
    .set_q("red shoe")
    .set_tags(vec!["a".into(), "b".into()])
    .set_sort(Sort::PriceAsc)
    .maybe_cursor(next_cursor)          // Option<String>: None leaves it out
    .send().await?;                     // ?q=red+shoe&tag=a&tag=b&sort=price_asc&limit=20
```

## Escape hatches

```rust
let prepared = api.search().set_q("x").prepare()?;      // native request, no I/O
println!("{prepared:?}");                               // authorization: <redacted>
let (client, mut request, decoder) = prepared.into_parts();
request.headers_mut().insert("x-trace", trace_id);
let page = decoder.decode(client.execute(request).await?).await?;
```
