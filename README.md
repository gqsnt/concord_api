# Concord V4

Concord is a small, typed, declarative layer over [`reqwest`](https://docs.rs/reqwest). One
`api!` declaration describes an HTTP API as a tree — **client → scopes → endpoints** — and
generates plain Rust types that build native `reqwest::Request`s and decode typed responses.

```rust
use concord::{Json, reqwest};

concord::api! {
    pub client Catalog(#[sensitive] header("x-api-key") key: String) {
        base "https://api.example.com/v2";

        scope region(region: Region) {
            subdomain "{region}";                      // https://eu.api.example.com/v2/...

            scope org(org: String) {
                path "orgs/{org}";

                GET products(query cursor?: String, query limit: u32 = 20) -> Page via Json {
                    path "products";
                }
                POST create(body product: NewProduct via Json) -> Product via Json { path "products"; }
            }
        }
    }
}

let api = Catalog::new(reqwest::Client::new(), CatalogConfig { key });
let page = api.region(Region::Eu).org("acme").products().set_limit(50).send().await?;
```

**What Concord does:** domain structure; required (`x: T`), optional (`x?: T`) and initialized
(`x: T = expr`) values with no meaning inferred from type names; compile-time composition of paths,
subdomains, query parameters and headers (`append`, `override`, `remove`); JSON/text/binary bodies
and responses; redaction of `#[sensitive]` values from Concord's own `Debug` and errors; native
escape hatches (`prepare()` → `reqwest::Request`, `send_raw()` → `reqwest::Response`).

**What Concord does not do:** authentication flows, retries, rate limiting, pagination, timeouts,
middleware or any transport configuration. Bring your own `reqwest::Client`; one `send()` is one
`Client::execute`.

## Documentation

- Language and API: the `concord` crate documentation (`cargo doc -p concord --open`).
- Normative semantics: [`v4/SEMANTICS.md`](v4/SEMANTICS.md).
- Coming from V3: [`docs/MIGRATION.md`](docs/MIGRATION.md).
- Design decisions, evidence and reviews: [`v4/`](v4/) (`DECISIONS.md`, `TEST_MATRIX.md`,
  `STATUS.md`, `evidence/`).
- Examples: [`examples/shapes`](examples/shapes) (three realistic API shapes with wire tests).

## Workspace

| Path | Contents |
|---|---|
| `crates/concord` | runtime and the `api!` entry point |
| `crates/concord-macros` | the `api!` compiler |
| `examples/shapes`, `examples/renamed` | downstream crates |
| `benches/baselines` | hand-written reqwest wrappers used as a comparison baseline |
| `benches/perf`, `benches/scale` | timing harness and synthetic-API generator |

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p concord-perf
```

Rust 1.85 or newer (edition 2024). Status: `4.0.0-alpha.1`, unpublished (`publish = false`).
V3 is preserved at tag `concord-v3-preserved`.

## License

MIT OR Apache-2.0, at your option.
