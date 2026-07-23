# Timeouts, retries, cancellation, features, and troubleshooting

> Audience: Basic  
> Covers: operation deadlines, bounded recovery, cancellation, Cargo features, and common failure modes

This page collects the runtime rules that affect every capability and the feature selections that make APIs available at compile time.

## Timeouts, retries, and cancellation

### Operation timeouts

The DSL supports finite `ms`, `s`, `m`, and `h` durations:

```rust
timeout 5s;
```

A child can set another duration or use `timeout off;`. A request can call `.timeout`, `.disable_timeout`, or `.inherit_timeout`.

The operation deadline covers result-determining work: hooks, body preparation, credential operations, limiter waiting, native send, response head, status handling, and acquisition up to the returned semantic output. Completion hooks run after the result is fixed and must bound their own work. Consumption of a returned `ByteStream` or `RecordStream` occurs after stream opening and is application-controlled.

### Retry selection

Retry is client-only:

```rust
retry off;
retry reqwest_default;
retry custom crate::bounded_retry;
```

`retry custom` requires `dangerous-custom-retry`.

Application traffic defaults to retry off. Reqwest-native retry, when enabled, remains inside one Concord attempt: it does not re-run hooks, body preparation, credential finalization, limiter acquisition, or completion.

### Authentication recovery

This is separate from general retry. A challenge-enabled credential may perform at most one recovery attempt. The same operation identity and original deadline are retained. The full attempt lifecycle runs again, including limiter reacquisition.

### Cancellation

Dropping a pending future cancels owned work:

- an ungranted limiter wait consumes no quota;
- provider or body futures are dropped;
- native request/body ownership is dropped;
- later pagination pages are not created;
- no detached cleanup is spawned by core.

Already completed effects are not rolled back. A successful limiter reservation is not refunded because a later step fails or the caller cancels.

### Provider retry

Provider HTTP has a separate reviewed retry selection and isolated client. It does not enter application hooks, application limiting, pagination, or custom application retry.

### What Concord cannot guarantee

Concord cannot cancel synchronous application code already running inside a trait method before it returns a future, bound completion hooks automatically, or roll back external side effects created by extensions.

## Cargo features

User-facing features are defined by the `concord` façade. It forwards semantic runtime features to core and concrete implementation features to extras; `concord_macros` does not mirror that graph or inspect selected implementation types.

### Defaults

```rust
default-tls
http2
rate-limit-governor
```

### Data formats

| Feature | Adds |
|---|---|
| `json` | extras-owned `Json<T>` and Serde JSON |
| `records` | `RecordDecoder`, `Records`, `RecordStream`, batching |
| `csv` | extras-owned typed CSV decoder plus core `records` |
| `ndjson` | extras-owned typed NDJSON decoder plus core `records`; does not enable general `json` |
| `oauth2-client` | extras-owned OAuth2 client-credentials provider; independent of the general `json` codec |
| `multipart` | Multipart request bodies through Reqwest |

### Compression

- `gzip`
- `brotli`
- `deflate`

These enable managed response decompression. Stream/body limits are applied to application bytes after decompression.

### HTTP

- `default-tls` supplies Rustls-based TLS support.
- `http2` enables HTTP/2 support.
- Without defaults, plain managed HTTP remains available, but HTTPS and HTTP/2 require their features.

### Rate limiting

`rate-limit-governor` lets the façade install the extras-owned process-local
governor as the generated default. Core itself retains only semantic
contracts, the explicit no-op, and a fail-closed fallback. Generated nonempty
lexical plans fail closed without a façade-installed or explicit functional
limiter (or a deliberate explicit no-op).

### Dangerous features

- `dangerous-raw-response`
- `dangerous-custom-retry`

Dangerous Cargo features are unified across the compiled binary and are not enabled by default.

### Isolation rules

- `records` alone has no Serde or CSV parser dependency.
- `csv` does not enable `ndjson` or general `json`.
- `ndjson` does not enable `csv` or general `json`.
- `concord_macros` has no edge to extras and no knowledge of JSON, CSV, NDJSON, or built-in pagination-controller names.

See the [Feature reference](../reference/features-limits-and-errors.md#feature-reference) for exact dependency implications and configuration examples.

## Troubleshooting

### Generated method is missing

Check:

- endpoint alias and snake-case normalization;
- reserved generated names;
- feature-gated response/body type availability;
- whether the method is a capability extension requiring the generated module prelude;
- whether pagination or raw-response capability is valid for that endpoint.

### Client build returns `LimiterUnavailable`

The generated client contains a nonempty lexical rate-limit plan but `rate-limit-governor` is disabled and no explicit limiter was supplied. Enable the feature, install a custom limiter, or deliberately install `NoopRateLimiter`.

### HTTPS returns `TlsUnavailable`

Enable `default-tls` or provide a configuration that supports the required URL. A no-default build does not silently send credential material over unsupported transport.

### `ResponseContract` or decode failure

Inspect status and sanitized headers. Confirm the declared response specification's `Accept` and content-type validation. For vendor media, implement a custom `DecodeBody` or `ResponseBodySpec`.

### Request body fails before hooks

Known exact/lower-bound metadata exceeded the request limit. This is intentional. Increase the finite limit only when the API contract justifies it.

### Pagination stops with repeated request

The controller produced a request structurally identical to an earlier page. Ensure bound continuation state actually changes host/path/query/header shaping. Authentication and bodies do not affect cycle identity.

### Pagination `HardCapExceeded`

The controller still returned another state at the cap. Use a larger hard cap or fix termination. Use `Take(n)` when truncation is intended.

### Rate limit appears shared across values

Generated rate limits are lexical and value-independent by design. Different region/item values share the declaration bucket. Use an explicit custom limiter or prepared explicit key for value-based partitioning.

### A stream fails after request success

Streaming endpoints complete at stream opening. Later read/decode errors are stream items. Handle them in the consumer loop; they are not returned from the original await after the stream is opened.

### Debug output lacks auth/body values

That is the intended redaction model. Use explicit semantic accessors only in controlled application code. Do not expect native request/response inspection.

### Cargo feature unexpectedly appears

Cargo unifies features across the compiled binary. Inspect the dependency graph with `cargo tree -e features`. Use a standalone fixture or separate package to prove a genuinely feature-disabled configuration.
