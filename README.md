# Concord v2.1

Concord compiles one client-shaped API contract into a domain-shaped Rust API, inert plans and descriptions, and one bounded managed runtime.

This checkout is the **locally integrated Concord v2.1 unpublished beta**,
version `2.1.0-beta.1`. Publication is disabled. Local beta integration and
stable publication are separate decisions; the single current disposition is
the [release-gate status](dev_doc/release_gate_status.md).

Windows and Linux are supported and mandatory for integration. macOS is not a
supported or release-gated platform for this decision.

The complete declaration below is compiled in the maintained examples crate.

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

The authored tree is the caller tree. Client construction is inert:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_builder -->
```rust
pub fn build_client() -> Result<CatalogApi, concord::ClientBuildError> {
    CatalogApi::builder()
        .set_tenant("acme".to_owned())
        .build()
}
```

One ordinary request follows the declared scope and endpoint shape:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_ordinary_request -->
```rust
pub fn ordinary_request(client: &CatalogApi) {
    let _request = client
        .regional("eu".to_owned())
        .item_summaries();
}
```

Runtime-discovered relative operations retain separate, fallible proof:

<!-- concord-source: crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs#runtime_endpoint -->
```rust
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path("records")
        .path("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .timeout(Duration::from_secs(5))
        .body_codec::<crate::request_response::specifications::ExampleCodec>(
            "request body".to_owned(),
        )
        .response::<crate::request_response::specifications::ExampleCodec, String>()
        .build()?;
    let _: Request<_> = endpoint.try_bind(&client)?;
    Ok(())
}
```

Core guarantees include opaque Rust payload types, explicit outer presence and repetition, fixed logical origins, endpoint-owned status/effect, one absolute operation deadline, managed retry/auth/rate/pagination lifecycles, and sanitized bounded evidence.

Start with the [documentation index](docs/README.md), [quick start](docs/quick_start.md), [normative DSL specification](docs/spec.md), or [v2.1 migration guide](docs/migration_v2_1.md). Maintainers should also read the [release-gate status](dev_doc/release_gate_status.md) and [claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

The complete maintained gate is:

```text
just verify
```

The workspace targets Rust 1.86. All intended public package manifests retain
`publish = false`. Historical packaging rehearsals do not establish a final
package name, registry ownership, or a license grant; those decisions and the
remaining external evidence are named in the canonical release-gate status.
