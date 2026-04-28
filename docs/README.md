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
3. [Style Guide](STYLE.md)
4. [DSL Overview](02-dsl-overview.md)
5. [Client Blocks](03-client.md)
6. [Scopes, Routes, and Endpoints](04-scopes-routes-endpoints.md)
7. [Generated Client Usage](05-generated-client.md)
8. [Policies: Headers, Query, Timeout](06-policies.md)
9. [Authentication](07-authentication.md)
10. [Bodies, Responses, and Mapping](08-bodies-responses-mapping.md)
11. [Retry](09-retry.md)
12. [Rate Limiting](10-rate-limiting.md)
13. [Caching](11-caching.md)
14. [Pagination](12-pagination.md)
15. [Runtime and Request Lifecycle](13-runtime.md)
16. [Extension Points](14-extension-points.md)
17. [Testing and Debugging](15-testing-debugging.md)
18. [DSL Reference](16-dsl-reference.md)
19. [Public API Boundary](PUBLIC_API.md)
20. [Macro Architecture](MACRO_ARCHITECTURE.md)
21. [Migration Notes](17-migration-notes.md)

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

api.auth_api()
    .login_for_session(LoginRequest {
        username: "alice".to_string(),
        password: "secret".to_string(),
    })
    .acquire_as_session()
    .await?;

let me = api.protected().me().await?;
```

## Migration

Removed syntax and replacement examples live in [Migration Notes](17-migration-notes.md).
The stable v4 user-facing docs only show the canonical v4 DSL.
