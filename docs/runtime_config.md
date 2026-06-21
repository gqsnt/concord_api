# Runtime Config

Generated clients expose advanced runtime configuration through `configure` and `configure_mut`.

Use `configure` when chaining from an owned client.

```rust
let api = PolicyApi::new()
    .configure(|cfg| {
        cfg.pagination_caps(Caps::default().max_pages(50).max_items(10_000));
    });
```

Use `configure_mut` when the client already exists.

```rust
let mut api = PolicyApi::new();
api.configure_mut(|cfg| {
    cfg.debug_level(DebugLevel::V);
    cfg.cache_store(cache_store.clone());
    cfg.rate_limiter(rate_limiter.clone());
});
```

Common configuration methods include:

- `debug_level(...)`
- `debug_sink(...)`
- `cache_store(...)`
- `rate_limiter(...)`
- `retry_policy(...)`
- `runtime_hooks(...)`
- `pagination_caps(...)`
- `max_auth_retries(...)`
- `max_response_body_bytes(...)`
- `no_response_body_limit()`

Debug sinks and runtime hooks are metadata-only. They may observe redacted URLs,
redacted headers, statuses, retry/cache/rate-limit events, and safe endpoint
metadata. They never receive request or response body bytes, and verbose debug
logging does not support live body previews.

## Deprecated Dev Body Capture

Live request/response body debug is not supported. `DebugSink`, stderr debug
output, and runtime hooks never receive body bytes.

For local generated-client debugging only, Concord exposes deprecated
`DevBodyCaptureConfig` through `RuntimeConfig::dev_body_capture(...)`. It is
disabled by default and marked deprecated because it can persist sensitive
response bytes to disk. It writes selected ordinary response bodies to local
files under the configured directory using generated safe filenames. It does
not capture request bodies, and it skips responses for authenticated requests
and auth/token acquisition paths by default.

Do not use dev body capture in production.

## Response Body Limits

Endpoint response bodies use a finite runtime read limit before endpoint decode.
The default is 16 MiB. Configure it with:

```rust
api.configure_mut(|cfg| {
    cfg.max_response_body_bytes(32 * 1024 * 1024);
});
```

`no_response_body_limit()` is the explicit advanced opt-out for unbounded
endpoint response reads:

```rust
api.configure_mut(|cfg| {
    cfg.no_response_body_limit();
});
```

Auth-internal HTTP/token response limits are separate from endpoint response
limits. Cache `max_body` controls only whether a response is eligible for cache
storage; it does not raise or lower the runtime body read limit used before
decode.

Per-request overrides stay on the pending request.

```rust
let value = api
    .text()
    .debug_level(DebugLevel::VV)
    .timeout(std::time::Duration::from_secs(2))
    .cache_refresh()
    .execute()
    .await?;
```
