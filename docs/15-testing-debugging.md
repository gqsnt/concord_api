# 15. Testing and Debugging

Concord is designed to be testable.

## Mock transport

Use `new_with_transport` to inject a test transport.

```rust
let api = users_api::UsersApi::new_with_transport(mock_transport);
```

This lets tests assert:

- URL;
- path;
- query;
- headers;
- body;
- rate-limit plan;
- metadata such as attempt/page index.

## Facade tests

Prefer testing generated facade usage:

```rust
let out = api.protected().me().await?;
```

This validates both:

- codegen facade;
- runtime request execution.

## Explicit endpoint tests

Use explicit endpoints when you need to test lower-level behavior:

```rust
let endpoint = endpoints::users::GetUser::new(42);

let out = api.request(endpoint)
    .debug_level(DebugLevel::VV)
    .execute()
    .await?;
```

## Auth tests

Endpoint-backed auth should be tested with the generated auth state:

```rust
api.auth_state()
   .session()
   .acquire(api.auth_api().login_for_session(...))
   .await?;

api.protected().me().await?;
```

Missing manual credentials should produce a useful error.

## Rate-limit tests

A recording limiter can assert generated rate-limit plans without waiting for real time.

Test that:

- inherited default buckets are present;
- endpoint-specific buckets are added;
- `rate_limit off` clears generated buckets;
- fresh cache hits skip limiter acquisition;
- retry attempts acquire again.

## Cache tests

Test:

- fresh hit skips transport;
- `no-store` is not stored;
- auth identity partitions cache keys;
- stale revalidation uses conditional headers;
- `cache_bypass` skips lookup and store;
- `cache_refresh` forces transport and updates cache.

## Pagination tests

Test:

- offset increments;
- page increments;
- cursor is omitted on first request when missing;
- cursor loop detection;
- max pages;
- max items;
- page index metadata.

## Debug levels

Global:

```rust
let api = users_api::UsersApi::new()
    .with_debug_level(DebugLevel::V);
```

Per request:

```rust
api.users()
   .get(42)
   .debug_level(DebugLevel::VV)
   .await?;
```

## UI / compile-fail tests

Good compile-fail tests should cover:

- unknown credential;
- unknown retry/cache/rate-limit profile;
- invalid rate-limit key;
- unsupported old DSL syntax;
- unsupported custom auth;
- unsupported auth any/all groups;
- unsafe retry without idempotency if that rule is enforced.
