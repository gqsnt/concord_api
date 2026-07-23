# Features, limits, errors, and compatibility

> Audience: All users  
> Purpose: configuration and operational reference

Use this page to select Cargo features, understand default finite limits, categorize failures, and check compiler and dependency compatibility.

## Feature reference

### `concord` façade features

| Feature | Default | Dependencies/capability |
|---|:---:|---|
| `default-tls` | yes | Rustls-backed HTTPS |
| `http2` | yes | Reqwest HTTP/2 |
| `rate-limit-governor` | yes | built-in process-local governor |
| `json` | no | extras-owned Serde + Serde JSON `Json<T>` codec |
| `records` | no | generic record contracts only |
| `csv` | no | extras-owned typed CSV decoder plus core `records` |
| `ndjson` | no | extras-owned typed NDJSON decoder plus core `records`; not general `json` |
| `oauth2-client` | no | temporary legacy OAuth2 client-credentials helper; independent of `json` |
| `multipart` | no | Reqwest multipart request support |
| `gzip` | no | gzip decompression |
| `brotli` | no | Brotli decompression |
| `deflate` | no | Deflate decompression |
| `dangerous-raw-response` | no | bounded raw successful response terminal |
| `dangerous-custom-retry` | no | custom native Reqwest retry builder |
| `dangerous-dev-tools` | no | internal deterministic development seam |

### Example configurations

Minimal plain HTTP:

```toml
concord = { path = "../concord", default-features = false }
```

Common JSON HTTPS client:

```toml
concord = {
    path = "../concord",
    default-features = false,
    features = ["default-tls", "http2", "json", "rate-limit-governor"]
}
```

CSV and NDJSON without general JSON endpoints:

```toml
features = ["default-tls", "records", "csv", "ndjson"]
```

### Feature absence

- Without TLS, protected HTTPS operations fail at build/preflight rather than silently downgrading.
- Without governor, nonempty generated plans require explicit limiter/no-op.
- Without JSON/multipart/record features, the corresponding façade types are unavailable.
- Dangerous features are binary-wide through Cargo unification.

### Macro graph

`concord_macros` depends on parsing/code-generation crates and URL/HTTP syntax support. It has no edge to `concord_extras` and does not recognize codec or pagination-controller type names.

## Limits and defaults

Defaults come from `RuntimeConfigBuilder` and `GovernorRateLimiter`.

### Runtime defaults

| Limit | Default | Configuration |
|---|---:|---|
| Request body | 16 MiB | `request_body_limit` |
| Buffered successful response | 16 MiB | `response_body_limit` |
| Streaming successful response | 16 MiB cumulative | `stream_response_limit` |
| Terminal status body | 64 KiB complete-only | `status_body_limit` / `disable_status_body_capture` |
| Provider request | 1 MiB | `provider_request_limit` |
| Provider response | 1 MiB | `provider_response_limit` |
| Pagination pages | 1,000 | `pagination_max_pages` |
| Pagination collected items | 100,000 | `pagination_max_items` |
| Pagination identity storage | 4 MiB | `pagination_identity_limit` |
| Auth failure cooldown cap | 60 s | `auth_failure_cooldown_cap` |
| Rate-limit cooldown cap | 60 s | `rate_limit_cooldown_cap` |
| Advanced rate key | 1,024 aggregate bytes | limiter/governor configuration |

Ordinary byte/page/item limits are positive nonzero types. Cooldown caps may be zero to disable retained cooldown duration.

### Record defaults

Built-in CSV and NDJSON undecoded staging: 1 MiB per record/line. A custom `RecordDecoder` selects its own positive finite bound.

### Governor defaults

| State | Default |
|---|---:|
| Quota entries | 4,096 |
| Cooldown entries | 4,096 |
| Idle quota TTL | 15 minutes |
| Key bytes | 1,024 |

Builder methods can replace these finite values. Zero key bytes permits only global keys. Quota/cooldown capacities and idle TTL must remain positive.

### First-excess behavior

- Known exact/lower-bound request excess may fail before hooks.
- Encoded request first excess is not sent.
- Buffered response first excess is not passed to the decoder.
- Stream first excess chunk is withheld.
- Record staging first excess is not retained or yielded.
- Oversized explicit rate key fails before state insertion/send.

### Choosing limits

Use remote-contract knowledge and application risk, not a single global maximum. A record export may need a large stream limit but still a small record-staging limit. A JSON endpoint may need a modest buffered limit and no stream increase.

## Error reference

### Top-level `ErrorCategory`

| Category | Meaning |
|---|---|
| `Configuration` | invalid client/request configuration |
| `RequestEncoding` | complete request codec failed |
| `RequestBody` | body production/transmission failed |
| `AuthenticationUnavailable` | no usable material |
| `AuthenticationProvider` | provider acquisition/refresh failed |
| `AuthenticationRejected` | final challenge rejected material |
| `RateLimit` | limiter preparation/acquisition failed |
| `Hook` | result-determining hook rejected operation |
| `Timeout` | operation deadline expired |
| `Connect` | connection establishment failed |
| `Transport` | other native transport failure |
| `HttpStatus` | final non-success status |
| `ResponseBody` | successful body read/stream failure |
| `ResponseDecoding` | successful buffered decode failed |
| `ResponseContract` | successful response violated declared contract |
| `Pagination` | controller/limit/cycle processing failed |
| `Internal` | safe internal invariant failure |

Enums are `#[non_exhaustive]`; include a fallback match arm.

### Authentication kinds

`Unavailable`, `Acquisition`, `Refresh`, `Rejected`, `InvalidConfiguration`, `RequestTooLarge`, `ResponseTooLarge`, `Internal`.

### Rate-limit kinds

`InvalidConfiguration`, `InvalidKey`, `Acquire`, `Internal`.

### Pagination kinds

`InvalidLimit`, `Controller`, `Overflow`, `RepeatedRequest`, `HardCapExceeded`, `PageCeilingExceeded`, `CollectedItemCeilingExceeded`, `IdentityStorageCeilingExceeded`.

### Client-build kinds

`MissingInput`, `InvalidBaseUrl`, `TlsUnavailable`, `HttpConfiguration`, `RuntimeConfiguration`, `LimiterUnavailable`, `Internal`.

### Configuration kinds

`InvalidValue`, `InvalidRoute`, `InvalidHeader`, `InvalidAuthentication`, `InvalidRateLimit`, `InvalidProxy`, `Internal`.

### Status response access

`ApiError::http_response()` returns `HttpResponseFailure`, which exposes status, sanitized headers, bounded `StatusBody`, and retry-after. `into_http_response()` transfers ownership.

### Safe messages

Concord errors intentionally avoid raw values. Extension error constructors may accept a safe message, but callers should keep it constant or value-free.

## Compatibility and MSRV

### Rust

The workspace declares:

```text
rust-version = 1.85
edition = 2024
```

Public APIs, feature combinations, examples, records, fixtures, and generated code are expected to compile on Rust 1.85.

### Version

Workspace version is `0.1.0`. The project is pre-1.0, so breaking public changes should increment the minor version. Patch releases should preserve documented public contracts.

### Generated contract

The façade bridge and macro share private generated contract version `16`. It protects their internal integration. An incompatible pairing fails during compilation. This version is not an application API version and should not be referenced by normal client code.

### Reqwest

The workspace pins Reqwest `0.13.4` with default features disabled. Core owns the normal dependency and exposes no native client/request/response type through stable APIs.

### Renamed dependencies

The macro resolves `concord_core` through Cargo package metadata, so downstream code may rename the dependency. Generated public paths remain stable.

### Feature compatibility

Cargo features are additive and binary-wide. Absence must be tested in isolated packages when workspace feature unification would otherwise enable a feature.

### Platforms

The implementation uses stable Rust, Tokio, Reqwest, and Rustls. This documentation does not claim support for targets on which those selected dependencies or required atomics/network capabilities are unavailable. Validate the intended target explicitly.
