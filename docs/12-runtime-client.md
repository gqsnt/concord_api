# 12. Runtime Client

The DSL generates endpoint types and a client wrapper. Runtime behavior comes from `concord_core`.

Most user code follows this shape:

```rust
let api = client::Client::new(true);
let value = api.request(client::endpoints::jsonplaceholder::posts::GetPost::new(1))
    .execute()
    .await?;
```

## Generated module layout

`client Client` generates a snake-case module named `client`.

```rust
api! {
    client Client {
        scheme: https,
        host: "example.com",
    }

    GET Ping -> Json<()>;
}
```

Use it like:

```rust
let api = client::Client::new();
api.request(client::endpoints::Ping::new()).execute().await?;
```

For `client RiotClient`, the module is `riot_client`.

Nested scopes also become nested endpoint modules. A scope such as `scope users { GET GetUser(...) ... }` generates `endpoints::users::GetUser`.

## Construction

Use `new(...)` for normal clients.

```rust
let api = users_api::UsersApi::new();
```

Required client vars and secrets become constructor arguments.

```rust
let riot = riot_client::RiotClient::new(api_key);
```

Use `new_with_transport(...)` in tests or when injecting a custom transport.

```rust
let api = users_api::UsersApi::new_with_transport(mock_transport);
```

For the core `ApiClient`, use `with_reqwest_client` or `with_transport` when working outside the generated wrapper.

## Sending a request

`request(endpoint)` returns `PendingRequest`.

```rust
let pending = api.request(endpoints::jsonplaceholder::posts::GetPost::new(1));
```

`execute()` sends and returns the decoded value.

```rust
let post = pending.execute().await?;
```

`execute_decoded()` sends and returns `DecodedResponse<T>`.

```rust
let response = api.request(endpoints::jsonplaceholder::posts::GetPost::new(1))
    .execute_decoded()
    .await?;

println!("{} {}", response.status, response.url);
```

## Pending request options

`PendingRequest` supports per-request options:

```rust
api.request(endpoints::jsonplaceholder::posts::GetPost::new(1))
    .debug_level(DebugLevel::VV)
    .timeout(core::time::Duration::from_secs(5))
    .execute()
    .await?;
```

Available methods include:

- `debug_level(DebugLevel::V | DebugLevel::VV)`
- `timeout(Duration)`
- `clear_timeout()`
- `inherit_timeout()`
- `attempt(u32)`
- `cache_default()`
- `cache_bypass()`
- `cache_refresh()`
- `execute()`
- `execute_decoded()`
- `paginate()` for paginated endpoints

## Runtime pipeline

A normal request follows this order:

1. Build request from route, policy, body, retry, rate-limit, and cache settings.
2. Run auth prepare.
3. Run cache `before_request`.
4. Return a fresh cache hit immediately.
5. Patch conditional headers for cache revalidation.
6. Join or lead inflight coordination.
7. Acquire rate-limit permits.
8. Run pre-send hooks.
9. Send through transport.
10. Classify response status and read the body.
11. Run post-response hooks.
12. Let auth inspect the response, then invalidate and/or retry per auth policy.
13. Update cache after auth accepts the response.
14. Decode and map the response.

Transport errors can run transport-error hooks, retry policy, and cache `after_error` fallback depending on configuration.

## Cloning clients

Generated clients are cloneable when their transport is cloneable.

Auth state is shared across clones. Secret setters rebuild auth state through that shared handle, so existing clones observe updated credentials.

```rust
let mut api = protected_api::ProtectedApi::new("tok1".to_string());
let clone = api.clone();

api.set_api_key("tok2");
clone.request(endpoints::Ping::new()).execute().await?;
```

The cloned request uses the updated secret.

Endpoint-backed manual credentials share state the same way:

```rust
let api = protected_api::ProtectedApi::new();
let clone = api.clone();

api.acquire_auth_session(endpoints::auth::LoginForSession::new(...)).await?;
assert!(clone.has_auth_session().await);

clone.clear_auth_session().await;
assert!(!api.has_auth_session().await);
```

Runtime components installed before cloning are shared by `Arc`: cache store, rate limiter, inflight registry, retry policy, runtime hooks, and debug sink. Setter methods replace the component on the handle you call them on; install process-wide components before cloning when every clone should use the same replacement.

## Debug output

Set debug globally:

```rust
let api = users_api::UsersApi::new()
    .with_debug_level(DebugLevel::V);
```

Or per request:

```rust
api.request(endpoints::GetPost::new(1))
    .debug_level(DebugLevel::VV)
    .execute()
    .await?;
```

`DebugLevel::V` logs request start and response status.

`DebugLevel::VV` also logs headers and formatted body previews.

Generated wrappers forward debug level and debug sink setters:

```rust
let api = users_api::UsersApi::new()
    .with_debug_sink(Arc::new(MyDebugSink))
    .with_debug_level(DebugLevel::V);
```

Use a custom `DebugSink` for tests, tracing integrations, or redaction policies that differ from the built-in stderr sink.

## Error inspection

`ApiClientError::HttpStatus` stores headers and rate-limit response action behind boxes to keep the error type reasonably sized. Pattern matching remains possible:

```rust
match err {
    ApiClientError::HttpStatus { status, headers, .. } => {
        eprintln!("status={status} retry-after={:?}", headers.get("retry-after"));
    }
    other => return Err(other),
}
```

For ordinary inspection, prefer accessors:

```rust
if err.http_status() == Some(http::StatusCode::TOO_MANY_REQUESTS) {
    if let Some(action) = err.rate_limit_response_action() {
        eprintln!("rate-limit action: {action:?}");
    }
}
```

`err.context()` returns the endpoint/method context for all error variants, and `err.http_headers()` returns response headers for HTTP status errors.

## Custom transport

A transport implements `Transport`.

```rust
pub trait Transport: Send + Clone + Sync + 'static {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>>;
}
```

A transport receives `BuiltRequest` with URL, headers, body, timeout, retry setting, rate-limit plan, cache setting, request metadata, and auth extensions.

Use custom transport for tests, alternate HTTP stacks, observability, or special networking requirements.

## Runtime hooks

Runtime hooks observe send lifecycle events.

```rust
pub trait RuntimeHooks: Send + Sync + 'static {
    fn pre_send<'a>(&'a self, ctx: PreSendHookContext<'a>)
        -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>>;

    fn post_response<'a>(&'a self, ctx: PostResponseHookContext<'a>)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

    fn transport_error<'a>(&'a self, ctx: TransportErrorHookContext<'a>)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}
```

Install hooks through generated wrappers or through the lower-level `concord_core::ApiClient` in integrations that use the core client directly. Generated wrappers forward the common runtime knobs: debug level, debug sink, pagination caps, rate limiter, cache store, inflight policy, runtime hooks, retry policy, and max auth retries.

Use hooks for metrics, tracing, auditing, or request blocking in `pre_send`.

## Inflight coordination

Inflight coordination deduplicates concurrent safe requests when configured.

```rust
let api = RateLimitDslApi::new()
    .with_inflight_policy(Arc::new(SafeMethodInflightPolicy));
```

The first matching request becomes the leader and sends transport. Followers wait for the shared result. Tests verify followers do not consume additional rate-limit permits.

## Runtime extension points

Core extension points include:

- `Transport` for the HTTP implementation.
- `RateLimiter` for rate-limit acquisition and cooldowns.
- `CacheStore` for caching.
- `RetryPolicy` for fallback retry behavior.
- `RuntimeHooks` for lifecycle instrumentation.
- `DebugSink` for debug logging.
- `InflightPolicy` for duplicate request suppression.

The DSL should describe API contract. Runtime extension points should handle environment-specific behavior.
