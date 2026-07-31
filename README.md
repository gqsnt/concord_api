# Concord

Concord describes an HTTP API as a typed domain tree and generates the Rust
client API that developers would want to read and use.

The declaration keeps domain structure, typed inputs, outputs, wire placement,
and policy in one reviewable contract. The generated client turns the same
structure into ordinary Rust methods, immutable scope snapshots, typed request
controls, and capability-specific terminals.

## The two product surfaces

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
```rust
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

The generated use follows the same tree. Required values are arguments;
optional and defaulted values become typed controls; pagination exists because
the endpoint declares it.

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

The endpoint author writes the domain operation and its local HTTP mapping. The
compiler resolves inherited behavior, generates the client surface, checks
cross-feature constraints, and sends every request form through one managed
runtime.

## Why Concord

### Read a large API as a domain

`client`, `scope`, and endpoint leaves show what belongs to what. Scopes are
immutable selected contexts, not cosmetic folders: they own visible values,
inherited request semantics, child declarations, and generated navigation.

### Add endpoints locally

Place a new endpoint in the correct scope, declare its typed signature, then
state only its local path, query, body, response, pagination, or policy delta.
The author does not rebuild scope types, request wrappers, encoding, credential
placement, limiter admission, retry, cancellation, or response acquisition.

### Derive guardrails from the contract

Because Concord knows tree position, value capabilities, body role, response
mode, authentication placement, rate dimensions, and pagination bindings, it
can generate the correct methods and reject invalid combinations. Unsupported
operations are absent or fail a sealed trait bound instead of becoming runtime
`Unsupported` branches.

### Extend semantics without bypassing them

Custom bodies, responses, providers, controllers, limiters, classifiers, hooks,
and runtime-defined endpoints participate through typed bounded contracts.
They do not receive unrestricted native application requests or responses.

## Implementation model

`concord::api!` parses a source-shaped contract, normalizes inheritance,
validates cross-site semantics, derives the generated Rust surface, and emits
against the public `concord` façade. The runtime receives typed resolved plans
and never interprets DSL syntax.

Request construction and planning are inert. Awaiting a request starts public
request shaping, hooks, body preparation, credential acquisition, rate-limit
admission, private authentication placement, HTTP, bounded response
acquisition, conversion, and completion.

## Use from this checkout

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord", features = ["json"] }
```

Concord requires Rust 1.86 or newer. The packages have `publish = false`.

## Documentation

Start with the [documentation index](docs/README.md).

| Goal | Read |
|---|---|
| Understand the product | [Overview](docs/overview.md) and [mental model](docs/mental_model.md) |
| Declare and call a client | [Quick start](docs/quick_start.md) |
| Model a large API | [Modeling APIs](docs/modeling_apis.md) and [Riot walkthrough](docs/riot_walkthrough.md) |
| Look up exact syntax | [DSL specification](docs/spec.md) |
| Understand execution | [Runtime model](docs/runtime.md) |
| Extend Concord | [Extensions](docs/extensions.md) |
| Maintain the project | [Maintainer internals](dev_doc/internals.md) |

## Validation

`cargo test --workspace` is not the repository gate. UI, MSRV, feature-isolation,
documentation, and fixture targets are configured separately, and several are
excluded from ordinary test runs. Use the maintained `just` recipes.

Fast local loop:

```text
just fmt
just docs-check
cargo nextest run --workspace
```

Before committing:

```text
just check
just test
just ui
just docs
just msrv
```

Full pre-release equivalent of CI:

```text
just verify
```

`just verify` runs `fmt`, `check`, `lint`, `test`, `ui`, `msrv`, `fixtures`,
`docs`, `deny`, `workflow`, `analyzer`, and `bench`. See
[maintainer internals](dev_doc/internals.md) for what each level owns.

Internal project. v1 final. Not published. See [FINAL.md](FINAL.md).
