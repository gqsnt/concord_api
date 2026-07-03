# Concord

Concord is a Rust API-tree DSL and contract compiler. It generates a facade-first typed client over a syntax-neutral, plan-based HTTP runtime.

## Quick Example

```rust
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

api! {
    client MinimalApi {
        base "https://api.example.com"
    }

    scope users {
        path ["users"]

        GET GetUser(id: u64)
            as get_user
            path [id]
            -> Json<User>
    }
}

# async fn run() -> Result<(), ApiClientError> {
let api = minimal_api::MinimalApi::new();
let user = api.users().get_user(42).await?;
# Ok(())
# }
```

## What You Get

- Typed facade navigation through `client`, `scope`, and endpoint methods.
- Required params as direct method arguments.
- Optional and defaulted request setters with `field`, `field_opt`, and `clear_field`.
- Direct `.await`, `.execute()`, `.execute_decoded()`, and explicit `.execute_raw()`.
- Explicit `.paginate().collect()`.
- Endpoint-backed credential acquisition with `.acquire_as_<credential>()`.
- OAuth2 client-credentials auth through generated token acquisition and bearer materialization.
- Advanced endpoint structs under `endpoints::*` for focused tests and request planning.

## Docs

- [Quick Start](docs/quick_start.md)
- [Mental Model](docs/mental_model.md)
- [DSL](docs/dsl.md) - complete public DSL reference
- [Generated Client](docs/generated_client.md)
- [Auth](docs/auth.md)
- [Pagination](docs/pagination.md)
- [Retry And Rate Limit](docs/retry_and_rate_limit.md)
- [Runtime Config](docs/runtime_config.md)
- [Advanced Endpoints](docs/advanced_endpoints.md)
- [Internals](docs/internals.md)

Developer architecture notes live in [`dev_doc/`](dev_doc/).

## Examples

The `concord_examples` crate contains current examples for:

- minimal client usage
- endpoint-backed auth
- OAuth2 client-credentials auth
- offset and cursor pagination
- custom pagination
- custom codecs
- retry and rate-limit policy profiles
- explicit endpoint requests
- compile-checked endpoint I/O examples covering Json, Text, stream, records, multipart, SSE, NoContent, and Bytes surfaces
- a compiled public DSL guide example in `concord_examples/src/docs_dsl.rs`
- compiled advanced DSL syntax examples in `concord_examples/src/docs_advanced_dsl.rs`
- a consolidated endpoint I/O example suite in `concord_examples/src/endpoint_io.rs`
- a Riot Web API large fixture in `concord_examples/src/riot.rs`
- a Data Dragon fixture in `concord_examples/src/ddragon.rs`

The Riot and Data Dragon fixtures include manual smoke functions gated by environment variables. They are not run by tests or normal example execution.
