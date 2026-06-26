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
- `pagination_detect_loops(...)`
- `max_auth_retries(...)`
- `max_response_body_bytes(...)`
- `no_response_body_limit()`

Pagination page and item termination is chosen per request with
`PaginationTermination`; there is no runtime-wide implicit page or item cap.
`pagination_detect_loops(...)` changes the default controller loop-key
detection setting for paginated calls. The runtime still enforces non-progress
detection for repeated logical page identities regardless of this setting.

Debug sinks and runtime hooks are metadata-only. They may observe redacted URLs,
redacted headers, statuses, retry/cache/rate-limit events, and safe endpoint
metadata. They never receive request or response body bytes. Concord does not
route body bytes through debug sinks, stderr logs, hooks, or callback APIs.
Rate-limit observation follows the same metadata-only boundary and remains
response-based rather than endpoint-success based. Rate-limit acquire and
response contexts stay body-free and raw-auth-free; query-auth URLs are
redacted structurally, and bearer/basic credentials do not appear in rate-limit
context surfaces.
Retry customization follows the same boundary: retry decisions are transport/status decisions only, and they run before stale fallback and endpoint decode/map. They do not see body bytes or raw auth material and cannot make a failed endpoint execution cache-admissible.

Custom cache stores receive the logical `BuiltRequest`. Protected requests are
eligible for cache only when the logical request carries a safe auth identity;
otherwise Concord bypasses cache lookup, stale fallback, and cache write. Cache
keys must use safe auth identity, not materialized auth headers, query-auth
values, tokens, secrets, or body bytes.

Reserved auth names are structural, not best-effort. Query-auth names are
rejected if a public query parameter already uses the same key, and
header-auth names are rejected case-insensitively if a public header already
uses the same name. Those collisions are rejected before cache lookup,
rate-limit acquisition, and transport.

## Deprecated Dev Body Capture

Live request/response body debug is not supported. `DebugSink`, stderr debug
output, and runtime hooks never receive body bytes.

For local generated-client debugging only, Concord exposes deprecated
`DevBodyCaptureConfig` through `RuntimeConfig::dev_body_capture(...)`. It is
disabled by default, may persist sensitive response bytes to local disk, and is
separate from cache, debug sinks, runtime hooks, and errors. It writes selected
ordinary response bodies to local files under the configured directory using
generated safe filenames. It does not capture request bodies, and it skips
responses for authenticated requests and auth/token acquisition paths by
default. It may capture the received body before endpoint decode so it remains
useful for local diagnosis of bad provider payloads and decode failures.

Do not use dev body capture in production. Release checks treat deprecated use
outside explicit tests as a failure.

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

When a response includes `Content-Length`, Concord rejects bodies above the
configured limit before reading any body chunks. Chunked or unknown-length
responses are still bounded: Concord reads them cumulatively and fails as soon
as the buffered body would exceed the limit. Body-limit failures are typed and
remain body-free in debug sinks, hooks, rate-limit metadata, and retry
metadata. `execute_raw()` follows the same response-body limit; it only bypasses
endpoint decode/map and cache lookup/store.

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
