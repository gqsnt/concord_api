# Concord v4 Documentation

Concord is a Rust API-client generator built around the `api!` macro.

You describe an HTTP API as a tree:

```text
client = root
scope = branch
endpoint = leaf
policy = inherited branch behavior
```

Concord then generates:

1. a typed client,
2. a tree-shaped facade (`api.scope().endpoint().await?`),
3. explicit endpoint structs for advanced use,
4. a runtime pipeline that handles routing, auth, cache, rate limits, retry, pagination, transport and decoding.

Most examples assume:

```rust
use concord_core::prelude::*;
use concord_macros::api;
```

## Recommended reading order

1. [Quick Start](00-quick-start.md)
2. [Mental Model](01-mental-model.md)
3. [DSL Overview](02-dsl-overview.md)
4. [Client Blocks](03-client.md)
5. [Scopes, Routes, and Endpoints](04-scopes-routes-endpoints.md)
6. [Generated Client Usage](05-generated-client.md)
7. [Policies: Headers, Query, Timeout](06-policies.md)
8. [Authentication](07-authentication.md)
9. [Bodies, Responses, and Mapping](08-bodies-responses-mapping.md)
10. [Retry](09-retry.md)
11. [Rate Limiting](10-rate-limiting.md)
12. [Caching](11-caching.md)
13. [Pagination](12-pagination.md)
14. [Runtime and Request Lifecycle](13-runtime.md)
15. [Extension Points](14-extension-points.md)
16. [Testing and Debugging](15-testing-debugging.md)
17. [DSL Reference](16-dsl-reference.md)
18. [Migration Notes](17-migration-notes.md)

## Canonical v4 style

```rust
api! {
    client SessionApi {
        base https "example.com"

        secret upstream_key: String

        credential upstream = api_key(secret.upstream_key)
        credential session = endpoint auth_api::LoginForSession
    }

    scope auth_api {
        POST LoginForSession(body: Json<LoginRequest>)
            -> Json<LoginResponse>
            map AccessToken {
                AccessToken::new(r.access_token)
            }
        {
            path ["login"]
            auth header "X-Upstream-Key" = upstream
        }
    }

    scope protected {
        auth bearer session

        GET Me
            as me
            path ["me"]
            -> Json<User>
    }
}
```

Usage:

```rust
let api = session_api::SessionApi::new("upstream-key".to_string());

api.auth_state()
    .session()
    .acquire(api.auth_api().login_for_session(LoginRequest {
        username: "alice".to_string(),
        password: "secret".to_string(),
    }))
    .await?;

let me = api.protected().me().await?;
```

## What this documentation does not cover as stable v4

These are intentionally not presented as stable v4 user-facing APIs:

- old `scheme:` / `host:` client syntax;
- old `auth { credential ... }` block syntax;
- old `use_auth HeaderAuth(...)` style;
- `auth any` / `auth all` groups;
- custom auth placement;
- cache storage tuning in the DSL;
- old `backoff none`;
- old rate-limit `response custom` and `route.host` syntax.
