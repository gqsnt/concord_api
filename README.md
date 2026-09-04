# Concord v2.1

Concord compiles one client-shaped API contract into a domain-shaped Rust API, inert plans and descriptions, and one bounded managed runtime.

This checkout is a **v2.1 implementation prototype**. It is not a release candidate until every applicable gate in [evidence_and_release_gates.md](docs/evidence_and_release_gates.md) passes, including the human usability gate.

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

        GET item(id: String) -> Item {
            id "catalog.item.get";
            path ["items", endpoint.id];
        }
    }
}
```

The authored tree is the caller tree. This complete example is compiled as part
of the maintained examples crate:

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

Start with the [documentation index](docs/README.md), [quick start](docs/quick_start.md), [normative DSL specification](docs/spec.md), or [v2.1 migration guide](docs/migration_v2_1.md). Maintainers should also read the [implementation plan](dev_doc/implementation_v2_1.md) and [claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

The complete maintained gate is:

```text
just verify
```

The workspace targets Rust 1.86 and keeps every package private (`publish = false`) during the prototype phase.
