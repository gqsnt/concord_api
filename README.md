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

The authored tree is the caller tree:

```rust
let client = CatalogApi::builder()
    .set_tenant("acme".to_owned())
    .build()?;

let item: Item = client.item("42".to_owned()).await?;

let intent = CatalogApi::unbound().item("42".to_owned());
let plan = client.plan(&intent)?;
let request = client.request(intent);
```

Runtime-discovered relative operations retain separate, fallible proof:

```rust
let endpoint = RuntimeEndpoint::builder(endpoint_id, Method::GET)
    .path("items")
    .path_value(RuntimeValue::sensitive(record_id))
    .response::<Json, Item>()
    .build()?;
let item = endpoint.try_bind(&client)?.await?;
```

Core guarantees include opaque Rust payload types, explicit outer presence and repetition, fixed logical origins, endpoint-owned status/effect, one absolute operation deadline, managed retry/auth/rate/pagination lifecycles, and sanitized bounded evidence.

Start with the [documentation index](docs/README.md), [quick start](docs/quick_start.md), [normative DSL specification](docs/spec.md), or [v2.1 migration guide](docs/migration_v2_1.md). Maintainers should also read the [implementation plan](dev_doc/implementation_v2_1.md) and [claim/evidence ledger](dev_doc/claim_evidence_ledger.md).

The complete maintained gate is:

```text
just verify
```

The workspace targets Rust 1.86 and keeps every package private (`publish = false`) during the prototype phase.
