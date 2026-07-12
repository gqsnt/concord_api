# Performance

This guide describes the performance-relevant knobs and trade-offs exposed by the current Concord runtime. It is mechanism-focused: it does not publish latency figures, thresholds, or benchmark results as expectations because those numbers are workload- and machine-specific. To measure your own workload, use the report-only benches in `perf/`.

For complete API details, cross-check this guide with [Runtime Config](runtime_config.md), [Retry And Rate Limit](retry_and_rate_limit.md), [Pagination](pagination.md), [Feature Matrix](features.md), and the repository [README](../README.md).

## Buffered vs Streaming Responses

Buffered response families, including codec-decoded responses and byte responses, read the response body into memory before decoding or returning it. This is the ergonomic path for small bounded payloads and formats that naturally decode from a complete body. The buffering path is implemented by the response entity adapters in `concord_core/src/io.rs` and the limited body reader in `concord_core/src/client/mod.rs`.

Streaming responses return a stream object and do not require the whole response body to be resident before the caller starts consuming it. `Stream<...>` uses the streaming attempt path in `concord_core/src/client/execute.rs`, with its adapter in `concord_core/src/io.rs`. Prefer it for large or long-lived byte transfers when bounded memory is more important than whole-body convenience.

Raw stream responses are guarded by the configured stream response limit. The limit applies while the stream is read, so it bounds total bytes accepted by the runtime even though the caller consumes incrementally.

## Body Limits

The advanced standard-body `LimitedBody` applies a single frame-aware counter
without collecting the body or spawning a forwarding task. It preserves chunk
boundaries and trailers, checks additions for overflow, and stops immediately
when a data frame would exceed the limit. `DynBody` reader adapters use the
existing 8 KiB default and bounded per-reader buffers; generic readers report
an unknown hint while files report their metadata length.

`RuntimeConfig::default()` sets three finite body limits, each to `16 * 1024 * 1024`, as defined in `concord_core/src/runtime/config.rs` and carried into `ClientRuntimeState` in `concord_core/src/runtime_state.rs`:

- `max_response_body_bytes(...)` protects buffered endpoint responses and raw execution responses.
- `max_stream_request_body_bytes(...)` protects streaming request bodies.
- `max_stream_response_body_bytes(...)` protects streaming responses.

Each limit has an explicit opt-out method: `no_response_body_limit()`, `no_stream_request_body_limit()`, and `no_stream_response_body_limit()`. Raising a limit allows larger payloads through that path; disabling a limit removes the byte cap for that path.

The buffered response path applies the common frame-aware limiter before bounded collection. When `no_response_body_limit()` disables the response-body limit, an unverified `Content-Length` does not become an allocation or limit authority; collection grows through normal buffer growth while reading.

## Retry And Rate Limit Interaction

Retry policy is bounded by configuration. `RetryConfig::max_attempts` is the absolute physical-send cap, counts the initial send, and accepts only `1..=3`. Policies classify outcomes; ordinary retries do not add a client-generated delay. Opted-in server-directed `Retry-After` response headers are bounded by `max_rate_limit_cooldown(...)` and are applied centrally in `concord_core/src/client/retry_flow.rs`.

Rate-limit response handling can also store cooldowns from provider metadata, including `Retry-After`, through the default governor runtime. Those cooldowns are capped by `max_rate_limit_cooldown(...)`. When an HTTP status error already produced a rate-limit action whose delay was stored for the limiter, `drive_attempts` zeroes the normal retry delay before the next attempt. That avoids waiting once in the rate limiter and again in retry sleep.

Authentication refresh resends consume the same `RetryConfig::max_attempts` capacity as transport and status retries. Auth rejection handling invalidates request-local auth preparation before retrying so refreshed credentials are prepared before the next attempt.

With the `rate-limit-governor` feature enabled, the default limiter enforces declared plans. Without that feature, the default limiter fails closed for non-empty rate-limit plans. Empty plans still pass. Install `NoopRateLimiter` explicitly when you intentionally want to opt out of enforcement.

## Rate-Limiter Key Cardinality

The default governor runtime keeps bounded state for both window limiters and response cooldowns. In `concord_core/src/rate_limit/governor_runtime.rs`, the default window-entry cap and cooldown-entry cap are finite, idle window entries are pruned by TTL, and expired cooldowns are pruned before the cooldown cap is enforced.

High-cardinality keys therefore do not grow the default limiter maps without bound: windows are pruned or the oldest entries are evicted to maintain the cap, while storing a new distinct cooldown beyond the cap fails closed. You should still choose rate-limit keys deliberately. Keys based on host, endpoint, method, tenant, or another stable bucket source are easier to reason about than keys containing request-unique values.

The empty-plan fast path bypasses window limiter creation. Empty plans can still observe active response cooldowns when a cooldown target applies to the request.

## Pagination

Pagination is collect-only. `PaginatedRequest::collect()` accumulates items into a `Vec`, so memory grows with collected items. The collect loop lives in `concord_core/src/request.rs`, and the public pagination contracts live in `concord_core/src/pagination.rs`.

Pagination state is per request. The runtime initializes the pagination controller for the current collection run, applies it to each page, advances it after successful page handling, and drops the state when `collect()` returns.

Loop detection uses per-request identity sets. The progress-key set is present only when loop detection is enabled, and the logical request-identity set is always per collection run. Their memory is bounded by the number of pages observed during that run and is freed when the request finishes.

Choose termination deliberately with `PaginationTermination`: hard page or item caps error when exceeded, while take-page or take-item modes stop cleanly. There is no runtime-wide implicit page or item cap.

## Debug And Observability Cost

`DebugLevel::None` is the default. It prevents debug sink request/response callbacks from emitting request starts, headers, or statuses. At verbose levels, the attempt path calls the debug sink for request and response status; at very verbose levels it also wraps headers in sanitized header views.

The attempt loop builds a sanitized URL string per attempt for metadata that is shared by debug, hooks, retry context, and transport-error reporting. Runtime hooks also run per attempt: `pre_send` before transport send, `post_response` after an HTTP response, and `transport_error` for initial send failures. The default hooks are no-op, but custom hooks are on the hot path.

Redaction is applied to observability surfaces. Header views use `SanitizedHeaders`, and URLs are rendered through `sanitize_url_for_debug`, so sensitive query keys and sensitive header names are redacted before they reach debug output or hook metadata. Hooks and debug sinks do not receive body bytes.

## Upload Chunk Size

`StreamBody::from_async_read(...)` and `StreamBody::from_file(...)` use an `8 * 1024` byte chunk size by default, as defined in `concord_core/src/stream_body.rs`. `StreamBody::from_async_read_with_chunk_size(...)` lets callers choose a non-zero chunk size.

Larger chunks reduce the number of stream items a transport must poll, at the cost of a larger per-stream buffer and larger emitted byte chunks. Smaller chunks reduce per-chunk buffer size but increase polling and per-chunk overhead. The async-read stream reuses its read buffer for the lifetime of the stream and copies each filled chunk into `Bytes` for transport.

## Auth Preparation Reuse

Auth preparation reuse is automatic and request-local. In `concord_core/src/client/execute.rs`, `drive_attempts` keeps an optional cached preparation for one request and clears it when auth rejection handling asks for a refreshed credential state.

The cache is enabled only when every prepared credential carries `AuthPreparationReuse::RequestLocal`. Generated clients set that typed signal for retry-stable static credential paths: API key, static bearer, and Basic credentials. OAuth2 client credentials and endpoint/manual credentials remain `Never`, so they prepare per attempt. The generated reuse decisions are emitted from `concord_macros/src/codegen/client.rs`, and the typed field lives in `concord_core/src/auth/plan.rs`.

Users do not need to opt in. Static generated credentials can reuse preparation across transport retries within the same request; auth rejection invalidates the cache before retry, so rejected credentials are prepared again.

## Build Footprint

For users minimizing dependency and compile footprint, `concord_core` supports `default-features = false`. The supported feature combinations are documented in [Feature Matrix](features.md).

The feature matrix describes the supported default and no-default dependency surfaces.

`reqwest` transport is mandatory. Default builds enable `reqwest` `stream`, `default-tls`, and `http2` transport capabilities (plus governor by default); no-default builds enable only `stream`. `gzip`, `brotli`, `deflate`, `cookies`, and `multipart` toggle optional transport capabilities and remain off by default in no-default builds; `rate-limit-governor` still controls limiter behavior. Disabling that feature changes the default limiter implementation; see [Feature Matrix](features.md) before using no-default builds in an application.

## Measuring Your Own Workload

The `perf/` package contains mock-based Criterion benches for local measurement. Run specific benches with commands such as:

```bash
cargo bench --manifest-path perf/Cargo.toml --bench attempt_pipeline
cargo bench --manifest-path perf/Cargo.toml --bench auth_runtime
cargo bench --manifest-path perf/Cargo.toml --bench allocation_counts
```

Treat results as machine-local evidence for your workload. They are useful for comparing branches or configuration choices on the same machine, not as portable latency guarantees.
