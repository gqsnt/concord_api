# Quick Start

Concord generates a typed Rust client from an API-tree contract.

## Minimal Contract

```rust
use concord_core::prelude::*;
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

api! {
    client MinimalApi {
        base https "api.example.com"
    }

    scope users {
        path ["users"]

        GET GetUser(id: u64)
            as get_user
            path [id]
            -> Json<User>
    }
}
```

## Minimal Call

```rust
let api = minimal_api::MinimalApi::new();
let user = api.users().get_user(42).await?;
```

Use `.execute_decoded()` when response metadata is needed:

```rust
let response = api.users().get_user(42).execute_decoded().await?;
let status = response.status();
let user = response.into_value();
```

## Next Steps

- Read `mental_model.md` for the client/scope/endpoint model.
- Read `dsl.md` for the contract syntax.
- Read `generated_client.md` for generated client usage.
