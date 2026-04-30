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
