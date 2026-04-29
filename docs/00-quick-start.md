# 0. Quick Start

This page shows the smallest useful Concord v5 API.

## Minimal client

```rust
use concord_core::prelude::*;
use concord_macros::api;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: u32,
    pub name: String,
}

api! {
    client UsersApi {
        base https "example.com"
    }

    scope users {
        path ["users"]

        GET GetUser(id: u32)
            as get
            path [id]
            -> Json<User>
    }
}
```

Use the generated tree facade:

```rust
async fn load_user() -> Result<User, ApiClientError> {
    let api = users_api::UsersApi::new();

    api.users()
        .get(42)
        .await
}
```

The generated module name is the snake-case form of the client name:

```text
client UsersApi -> users_api
```

## What was generated

For the example above, Concord generates:

```rust
users_api::UsersApi
users_api::endpoints::users::GetUser
api.users().get(id)
api.request(endpoint).execute()
```

The facade is the normal path:

```rust
api.users().get(42).await?;
```

The explicit endpoint path remains useful for generic or low-level code:

```rust
api.request(users_api::endpoints::users::GetUser::new(42))
    .execute()
    .await?;
```

## API tree

Write APIs as trees:

```rust
api! {
    client Api {
        base https "example.com"
    }

    scope v1 {
        path ["v1"]

        scope users {
            path ["users"]

            GET GetUser(id: u64)
                as get
                path [id]
                -> Json<User>
        }
    }
}
```

Usage mirrors the tree:

```rust
api.v1().users().get(1).await?;
```

## Shared auth

```rust
api! {
    client ProtectedApi {
        base https "example.com"

        secret api_key: String
        credential key = api_key(secret.api_key)
    }

    scope protected {
        path ["api"]
        auth header "X-Api-Key" = key

        GET Me
            as me
            path ["me"]
            -> Json<User>
    }
}
```

Usage:

```rust
let api = protected_api::ProtectedApi::new("secret".to_string());
let me = api.protected().me().await?;
```

## Shared retry

```rust
client Api {
    base https "example.com"

    default {
        retry read
    }

    retry read {
        max_attempts 2
        methods [GET, HEAD]
        on [429, 500, 502, 503, 504]
        retry_after
    }
}
```

## Shared rate limit

```rust
client RiotClient {
    base https "riotgames.com"

    rate_limit app {
        bucket application by [host] {
            500 / 10s
            30000 / 10m
        }
    }

    default {
        rate_limit app
    }
}
```

## Rule of thumb

Put shared behavior as high as possible:

```text
client     global defaults
scope      API family defaults
endpoint   one request contract
```
