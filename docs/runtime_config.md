# Runtime Config

Generated clients expose advanced runtime configuration through `configure` and `configure_mut`.

Use `configure` when chaining from an owned client.

```rust
let api = PolicyApi::new()
    .configure(|cfg| {
        cfg.pagination_detect_loops(true);
    });
```

Use `configure_mut` when the client already exists.

```rust
let mut api = PolicyApi::new();
api.configure_mut(|cfg| {
    cfg.debug_level(DebugLevel::V);
    cfg.rate_limiter(rate_limiter.clone());
});
```

Common configuration methods include:

- `debug_level(...)`
- `debug_sink(...)`
- `rate_limiter(...)`
- `retry_policy(...)`
- `runtime_hooks(...)`
- `pagination_detect_loops(...)`
- `max_auth_retries(...)`
- `max_retry_delay(...)`
- `max_rate_limit_cooldown(...)`
- `max_response_body_bytes(...)`
- `no_response_body_limit()`

## Defaults And Precedence

`RuntimeConfig::default()` currently uses:

| Setting | Default |
| --- | --- |
| `debug_level` | `DebugLevel::None` |
| `debug_sink` | stderr sink |
| `runtime_hooks` | no-op hooks |
| `rate_limiter` | default rate limiter for the enabled feature set |
| `retry_policy` | no retry |
| `max_auth_retries` | `8` |
| `pagination_detect_loops` | `true` |
| `max_retry_delay` | `Duration::from_secs(60)` |
| `max_rate_limit_cooldown` | `Duration::from_secs(60)` |
| `max_response_body_bytes` | `Some(16 * 1024 * 1024)` |
| `dev_body_capture` | disabled |

With `rate-limit-governor` enabled, the default rate limiter enforces declared plans. With `default-features = false`, the default rate limiter fails closed for non-empty declared plans so they do not disappear silently. Empty plans still succeed. `NoopRateLimiter` remains an explicit opt-out for callers that intentionally want no enforcement.

Configuration precedence is:

```text
RuntimeConfig defaults
-> client configuration through configure/configure_mut or setter methods
-> endpoint policy emitted by the DSL or advanced endpoint
-> pending-request overrides where that API exists
```

Pending-request overrides exist for request options such as debug level, timeout, and attempt. There is no per-request response-body-limit, hook, rate-limiter, retry-policy, or auth-retry override in v1.

Rust borrowing prevents mutating one client instance while a request borrowed from that same instance is executing. Cloned clients use clone-on-write runtime state: configuring one clone does not change an already-cloned client or an in-flight request running on that clone. Later requests on the reconfigured clone use the new configuration.

Pagination page and item termination is chosen per request with `PaginationTermination`; there is no runtime-wide implicit page or item cap. `pagination_detect_loops(...)` changes the default controller loop-key detection setting for paginated calls. The runtime still enforces non-progress detection for repeated logical page identities regardless of this setting.

Debug sinks and runtime hooks are metadata-only. They receive sanitized metadata views: URLs are redacted before callback invocation, request and response headers are wrapped in a redacted header view, and they may observe statuses, retry events, rate-limit events, and endpoint metadata. They never receive request or response body bytes, and they cannot observe raw auth material.

The transport boundary is unchanged. The runtime still materializes raw request headers, query auth, and bodies only when building the `TransportRequest` that goes to the transport implementation.

Retry customization follows the same boundary: retry decisions are transport/status decisions only, and they run after response classification, hook observation, rate-limit observation, and auth rejection handling, and before endpoint response decoding. They do not see body bytes or raw auth material.

Retry delays are capped by `max_retry_delay(...)`, and rate-limit response cooldowns are capped by `max_rate_limit_cooldown(...)`. The defaults are finite, and over-cap remote or custom delays fail closed instead of sleeping or storing a cooldown.

Reserved auth names are structural, not best-effort. Query-auth names are rejected if a public query parameter already uses the same key, and header-auth names are rejected case-insensitively if a public header already uses the same name. Those collisions are rejected before rate-limit acquisition and transport.

## Dev Body Capture

Live request and response body debug is not supported. `DebugSink`, stderr debug output, and runtime hooks never receive body bytes.

For local generated-client debugging only, Concord exposes deprecated `DevBodyCaptureConfig` through `RuntimeConfig::dev_body_capture(...)`. It is disabled by default, may persist ordinary response bytes to local disk, and is separate from debug sinks, runtime hooks, and errors. It writes selected response bodies to local files under the configured directory using generated safe filenames. It does not capture request bodies, and it skips protected auth-bearing requests and auth endpoint traffic by default.

Do not use dev body capture in production. Release checks treat deprecated use outside explicit tests as a failure.

## Response Body Limits

Endpoint response bodies use a finite runtime read limit before endpoint decode. The default is 16 MiB. Configure it with:

```rust
api.configure_mut(|cfg| {
    cfg.max_response_body_bytes(32 * 1024 * 1024);
});
```

`no_response_body_limit()` is the explicit advanced opt-out for unbounded endpoint response reads:

```rust
api.configure_mut(|cfg| {
    cfg.no_response_body_limit();
});
```

Auth-internal HTTP and token responses use their own read limit. When a response includes `Content-Length`, Concord rejects bodies above the configured limit before reading any body chunks. Chunked or unknown-length responses are still bounded: Concord reads them cumulatively and fails as soon as the buffered body would exceed the limit. Body-limit failures are typed and remain body-free in debug sinks, hooks, rate-limit metadata, and retry metadata. `execute_raw()` follows the same response-body limit; it only bypasses endpoint response decoding.

Per-request overrides stay on the pending request.

```rust
let value = api
    .text()
    .debug_level(DebugLevel::VV)
    .timeout(std::time::Duration::from_secs(2))
    .execute()
    .await?;
```

These pending-request overrides do not mutate the client default and do not leak into later requests.
