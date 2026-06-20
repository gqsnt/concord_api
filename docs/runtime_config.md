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
- `debug_body(...)`
- `debug_sink(...)`
- `cache_store(...)`
- `rate_limiter(...)`
- `retry_policy(...)`
- `runtime_hooks(...)`
- `pagination_caps(...)`
- `max_auth_retries(...)`
- `max_response_body_bytes(...)`
- `no_response_body_limit()`

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
