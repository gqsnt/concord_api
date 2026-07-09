# Runtime Config

Generated clients expose advanced runtime configuration through `configure`.

See [Security Model](security_model.md) for the boundary between safe runtime defaults, advanced configuration, and dangerous dev-only capture.

Use `configure` on a mutable client.

```rust
let mut api = PolicyApi::new();
api.configure(|cfg| {
    cfg.pagination_detect_loops(true);
    cfg.debug_level(DebugLevel::V);
    cfg.rate_limiter(rate_limiter.clone());
});
```

Generated client wrappers forward the same runtime mutation surface through `configure(...)`; some wrapper types also expose a `configure_mut(...)` convenience method, but the underlying core client API is `configure(...)`.

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
| `dev_body_capture` | disabled, feature-gated behind `dangerous-dev-tools` |

With `rate-limit-governor` enabled, the default rate limiter enforces declared plans. With `default-features = false`, the default rate limiter fails closed for non-empty declared plans so they do not disappear silently. Empty plans still succeed. `NoopRateLimiter` remains an explicit opt-out for callers that intentionally want no enforcement.

Configuration precedence is:

```text
RuntimeConfig defaults
-> client configuration through configure or setter methods
-> endpoint policy emitted by the DSL or advanced endpoint
-> pending-request overrides where that API exists
```

Pending-request overrides exist for request options such as debug level, timeout, and attempt. There is no per-request response-body-limit, hook, rate-limiter, retry-policy, or auth-retry override in v1.

Rust borrowing prevents mutating one client instance while a request borrowed from that same instance is executing. Cloned clients use clone-on-write runtime state for configuration, but auth state is shared across clones. Configuring one clone does not change an already-cloned client or an in-flight request running on that clone, while auth-state mutations on one clone can be observed by other clones that share the same auth-state handle. Later requests on the reconfigured clone use the new runtime configuration.

Pagination page and item termination is chosen per request with `PaginationTermination`; there is no runtime-wide implicit page or item cap. `pagination_detect_loops(...)` changes the default controller loop-key detection setting for paginated calls. The runtime still enforces non-progress detection for repeated logical page identities regardless of this setting, and the resulting diagnostics only expose safe pagination metadata rather than raw cursor or progress-key contents.

Debug sinks and runtime hooks are metadata-only observers. They receive sanitized metadata views: URLs are redacted before callback invocation, request and response headers are wrapped in a redacted header view, and they may observe statuses, retry events, rate-limit events, and endpoint metadata. Rate-limit response observation also uses sanitized header views, so sensitive response headers are redacted before callback access while `Retry-After` and other non-sensitive headers remain available. `pre_send` runs after rate-limit acquisition and before raw auth transport materialization, `post_response` runs after an HTTP response is received and before response body read and endpoint decode, and `transport_error` only observes initial transport-send failures. Hooks never receive request or response body bytes, raw auth material, or raw secret values, and high-volume debug can add measurable overhead.

The transport boundary is unchanged. The runtime still materializes raw request headers, query auth, and bodies only when building the `TransportRequest` that goes to the transport implementation. Auth collision checks happen before rate-limit acquisition, hooks, debug, and transport side effects.

Retry customization follows the same boundary: retry decisions are transport/status decisions only, and they run after response classification, hook observation, rate-limit observation, and auth rejection handling, and before endpoint response decoding. They do not see body bytes or raw auth material.

Retry delays are capped by `max_retry_delay(...)`, and rate-limit response cooldown durations are capped by `max_rate_limit_cooldown(...)`. The default governor runtime also bounds the number of stored cooldown entries and fails closed if storing a new distinct cooldown would exceed that entry cap after expired entries are pruned. Advanced callers that need a different fixed cooldown-entry cap can install an explicitly configured governor limiter through `rate_limiter(...)`:

```rust
use concord_core::advanced::GovernorRateLimiter;
use std::sync::Arc;

api.configure(|cfg| {
    cfg.rate_limiter(Arc::new(
        GovernorRateLimiter::new().with_max_cooldown_entries(1024),
    ));
});
```

The defaults are finite, and over-cap remote or custom delays fail closed instead of sleeping or storing a cooldown.

Reserved auth names are structural, not best-effort. Query-auth names are rejected if a public query parameter already uses the same key, and header-auth names are rejected case-insensitively if a public header already uses the same name. Those collisions are rejected before rate-limit acquisition and transport.

## Dangerous Dev Body Capture

`dangerous-dev-tools` enables the deprecated dev body capture configuration API under `concord_core::dangerous`, but it does not turn capture on by itself.

Live request and response body debug is not supported. `DebugSink`, stderr debug output, and runtime hooks never receive body bytes.

For local generated-client debugging only, Concord exposes deprecated `concord_core::dangerous::DevBodyCaptureConfig` through `RuntimeConfig::dev_body_capture(...)` behind `dangerous-dev-tools`. It is deprecated, disabled by default, dev-only, and local-file-only. It writes raw selected response bytes to local disk with no redaction. It never captures request bodies, and it skips protected auth-bearing requests and auth endpoint traffic by default. `max_bytes` is a capture-size filter, not redaction and not a truncation guarantee.

Dev body capture is separate from debug sinks, runtime hooks, stderr debug output, public errors, retry metadata, and rate-limit metadata. Do not enable it in production, CI logs, CI artifacts, shared directories, user-visible support bundles, or any environment without controlled local filesystem permissions. Callers are responsible for local directory permissions, retention, cleanup, and artifact exclusion.

Do not use dev body capture in production. Release checks treat deprecated use outside explicit tests as a failure.

See [Security Model](security_model.md) for the consumer guidance around dangerous features and local capture artifacts.

## Response Body Limits

Endpoint response bodies use a finite runtime read limit before endpoint decode. The default is 16 MiB. Configure it with:

```rust
api.configure(|cfg| {
    cfg.max_response_body_bytes(32 * 1024 * 1024);
});
```

`no_response_body_limit()` is the explicit advanced opt-out for unbounded endpoint response reads:

```rust
api.configure(|cfg| {
    cfg.no_response_body_limit();
});
```

Auth-internal HTTP and token responses use their own read limit. When a response includes `Content-Length`, Concord rejects bodies above the configured limit before reading any body chunks. Chunked or unknown-length responses are still bounded: Concord reads them cumulatively and fails as soon as the buffered body would exceed the limit. Body-limit failures are typed and remain body-free in debug sinks, hooks, rate-limit metadata, and retry metadata. `#[cfg(feature = "dangerous-raw-response")] execute_raw_response()` follows the same response-body limit; it only bypasses endpoint response decoding.

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
