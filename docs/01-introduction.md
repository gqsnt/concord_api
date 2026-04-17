# 1. Introduction

Concord splits an API client into two layers.

The first layer is the DSL. It describes clients, scopes, routes, endpoints, request bodies, response codecs, and inherited policies.

The second layer is the runtime client in `concord_core`. It turns an endpoint value into a request, applies auth, checks cache, coordinates inflight requests, acquires rate-limit permits, sends through transport, retries when configured, stores cache responses, decodes the body, and returns the typed result.

## A minimal client

```rust
use concord_core::prelude::*;
use concord_macros::api;

mod models {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct User {
        pub id: u32,
        pub name: String,
    }
}

api! {
    client UsersApi {
        scheme: https,
        host: "example.com",
    }

    scope users {
        path["users"]

        GET GetUser {
            params { id: u32 }
            path[id]
            -> Json<models::User>;
        }
    }
}

async fn load_user() -> Result<models::User, ApiClientError> {
    let api = users_api::UsersApi::new();
    api.request(users_api::endpoints::GetUser::new(42))
        .execute()
        .await
}
```

The generated module name is the snake-case form of the client name. `client UsersApi` generates `users_api::UsersApi` and endpoint types under `users_api::endpoints`.

## The core vocabulary

A `client` block defines the base scheme, host, shared variables, secrets, and default policies.

A `scope` groups route segments and policies. Scopes can be nested, so shared path prefixes, host labels, auth, retry, cache, and rate-limit settings can be written once.

An endpoint declares the HTTP method, endpoint name, endpoint parameters, route additions, policy overrides, optional body, response codec, and optional response mapping.

A policy is inherited from client to scope to endpoint. Endpoint-level settings are the most specific. Some settings can patch inherited state, replace it, or turn it off.

## What Concord generates

For each endpoint, Concord generates a struct with a constructor and builder-style setters for optional or defaulted parameters.

```rust
let endpoint = users_api::endpoints::GetUser::new(42);
let value = api.request(endpoint).execute().await?;
```

For a parameter declared as optional or defaulted, the generated endpoint usually starts from `new()` or from required arguments, then exposes a setter with the same Rust field name.

```rust
GET ListUsers {
    params {
        page?: u32,
        trace: bool = false
    }
    query {
        "page" = page,
        "trace" = trace
    }
    -> Json<Vec<User>>;
}
```

```rust
api.request(endpoints::ListUsers::new().page(2).trace(true))
    .execute()
    .await?;
```

## Request lifecycle

The runtime request pipeline is intentionally ordered:

1. Build URL, headers, query, body, timeout, retry, rate-limit, and cache policy.
2. Prepare authentication.
3. Ask the cache store before sending.
4. Return immediately for a fresh cache hit.
5. Add conditional headers for stale cache revalidation.
6. Coordinate inflight duplicate requests.
7. Acquire rate-limit permits.
8. Send through transport.
9. Classify the response and handle retries.
10. Let auth inspect responses and decide invalidation and retry.
11. Update cache after accepted responses.
12. Decode the body and apply endpoint mapping.

This ordering matters. A fresh cache hit skips inflight coordination, rate-limit acquisition, retry, and transport. Stale revalidation still uses inflight, rate-limit, retry, and transport.

## Files worth reading

The examples are the best executable reference:

- `concord_examples/src/test_api.rs` shows a small JSONPlaceholder-style API.
- `concord_examples/src/auth_session.rs` shows endpoint-backed manual session auth (`Endpoint(...)` + `acquire_auth_*`).
- `concord_examples/src/riot.rs` shows a large nested real-world API with auth, host routing, rate-limit profiles, and pagination.
- `concord_examples/tests/*` covers behavior for routing, headers, query, auth, retry, rate limit, cache, pagination, response mapping, body handling, and status constraints.
