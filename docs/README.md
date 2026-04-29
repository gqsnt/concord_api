# Concord v5 Documentation

Concord is a Rust API-client generator built around the `api!` macro.

You describe an HTTP API as a tree:

```text
client = root
scope = branch
endpoint = leaf
policy = inherited branch behavior
```

Concord generates a typed client, a tree-shaped facade, explicit endpoint structs for advanced use, and a plan-based runtime pipeline for routing, auth, cache, rate limits, retry, pagination, transport, and decoding.

Most examples assume:

```rust
use concord_core::prelude::*;
use concord_macros::api;
```

## Recommended Reading Order

1. [Quick Start](00-quick-start.md)
2. [Mental Model](01-mental-model.md)
3. [Style Guide](STYLE.md)
4. [DSL Overview](02-dsl-overview.md)
5. [Generated Usage](03-generated-usage.md)
6. [Runtime Config](04-runtime-config.md)
7. [Authentication](05-auth.md)
8. [Pagination](06-pagination.md)
9. [Cache, Retry, Rate Limit](07-cache-retry-rate-limit.md)
10. [Client Blocks](03-client.md)
11. [Scopes, Routes, and Endpoints](04-scopes-routes-endpoints.md)
12. [Detailed Generated Client Usage](05-generated-client.md)
13. [Policies: Headers, Query, Timeout](06-policies.md)
14. [Detailed Authentication](07-authentication.md)
15. [Bodies, Responses, and Mapping](08-bodies-responses-mapping.md)
16. [Retry](09-retry.md)
17. [Rate Limiting](10-rate-limiting.md)
18. [Caching](11-caching.md)
19. [Detailed Pagination](12-pagination.md)
20. [Runtime and Request Lifecycle](13-runtime.md)
21. [Extension Points](14-extension-points.md)
22. [Testing and Debugging](15-testing-debugging.md)
23. [DSL Reference](16-dsl-reference.md)
24. [Public API Boundary](PUBLIC_API.md)
25. [Macro Architecture](MACRO_ARCHITECTURE.md)

## Canonical v5 Style

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
            path ["login"]
            auth header "X-Upstream-Key" = upstream
            -> Json<LoginResponse>
            map AccessToken {
                AccessToken::new(r.access_token)
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
