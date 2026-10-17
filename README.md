# Concord v3

Concord is a Rust library and `api!` declaration language for **typed, managed
HTTP clients**. You describe one client-shaped domain tree. Callers hold a
client or scope, construct an inert typed request, select a terminal and
receive a typed result, through one bounded managed runtime.

This checkout is the **Concord v3 unpublished beta**, version `3.0.0-beta.1`.
Publication is disabled. Local beta integration and stable publication are
separate decisions; the single current disposition is the
[release-gate status](dev_doc/release_gate_status.md).

Windows and Linux are supported and mandatory for integration. macOS is not a
supported or release-gated platform for this decision.

## One declaration, one caller tree

The declaration below is compiled in the maintained examples crate. Its host is
a fixture, not a live service.

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

The authored tree is the caller tree. Building the client performs no I/O and
reports every missing input in one `ClientBuildError`:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_builder -->
```rust
pub fn build_client() -> Result<CatalogApi, concord::ClientBuildError> {
    CatalogApi::builder().set_tenant("acme").build()
}
```

An ordinary `.await` executes exactly one managed operation and returns one
decoded page. It never paginates:

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

`query category?: String` is optional: `set_category(v)` selects a payload,
`replace_category(Option<String>)` replaces the slot exactly,
`set_category_if_some(None)` keeps the current value and `omit_category()`
removes it. `query offset: u64 = 0` is initialized: `reset_offset()` restores
the retained initializer without evaluating it again. More pages are always an
explicit choice: `.collect_prefix(n)`, `.collect_all(NonZeroU32)` or the lazy
`.pages(n)`.

## What the managed runtime guarantees

- **One meaning per spelling.** A bare name means the current owner only;
  ancestors are qualified (`client.tenant`, `regional.region`). Keys already
  owned by an ancestor need an explicit `replace`, `when_present`, `append` or
  `remove`.
- **Fixed logical origins.** The base owns scheme, host suffix, port and base
  path. Routes are relative templates; DNS labels are typed list atoms. No
  request or runtime operation can supply another origin.
- **Inert requests and plans.** Holding a request or planning it
  (`request.plan()`) never prepares a body, obtains credentials, reserves quota
  or sends.
- **Truthful outcomes.** Errors carry the failure stage and aggregate dispatch
  knowledge (`NotDispatched`, `PossiblyDispatched`, `ResponseObserved`). A page
  bound is never reported as exhaustion and an acquired stream is never
  reported as complete.
- **Explicit authority.** Credentials, rate policies, retries, pagination and
  streams have separate, finite, documented authorities. Sensitive values are
  sent but withheld from plans, errors and observers.

## Cargo features

The facade `concord` crate's features. Features add integrations; they never
change the meaning of an enabled declaration.

| Feature | Default | Adds |
|---|---|---|
| `default-tls` | yes | TLS for `https` bases (required to build a client with an `https` base) |
| `http2` | yes | HTTP/2 transport support |
| `rate-limit` | yes | The first-party `rate_limit::LocalRateLimiter` |
| `json` | no | The `Json` representation (serde) |
| `oauth2-client` | no | The `auth::OAuth2ClientCredentials` provider |
| `records` | no | Framed record streams (`ItemStream`) |
| `csv` | no | CSV records (implies `records`) |
| `ndjson` | no | NDJSON records (implies `records`) |
| `multipart` | no | Request-only `body::Multipart` bodies |
| `gzip` | no | gzip response decompression |
| `brotli` | no | Brotli response decompression |
| `deflate` | no | deflate response decompression |

## Documentation

Start with the [documentation index](docs/README.md) and the
[quick start](docs/quick_start.md). The [DSL specification](docs/spec.md) is
normative. [V3 is the only supported product version](docs/version_policy.md).
Maintainers should also read the
[release-gate status](dev_doc/release_gate_status.md) and the
[claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

## Verification

The complete maintained gate is:

```text
just verify
```

The documentation truth check alone is:

```text
cargo run -p concord-xtask -- docs-check
```

The workspace targets Rust 1.86 (edition 2024). All intended public package
manifests retain `publish = false`. The source license is selected below. Final registry names, ownership and
publication remain separate release actions recorded in the canonical
release-gate status.

## License

Concord is available under either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option
(`MIT OR Apache-2.0`). Third-party dependencies retain their own licenses.
