# Runtime configuration and limits

A built client owns an immutable snapshot of transport and runtime
configuration. Trusted configuration is chosen once, on the builder, through
two closures over validated facades; requests can only narrow it (§8.3,
§20.4).

## The two configuration closures

```text
builder.configure_http<F>(self, configure: F) -> Self
    where F: FnOnce(HttpConfigBuilder) -> HttpConfigBuilder
builder.configure_runtime<F>(self, configure: F) -> Self
    where F: FnOnce(RuntimeConfigBuilder) -> RuntimeConfigBuilder
```

Neither closure receives a raw native client builder, so no redirect, cookie or
upgrade escape hatch exists. Closures apply in call order. Every setter consumes
and returns the builder; an invalid value is recorded and the final `build()`
reports all independent errors in one `ClientBuildError`. A repeated trusted
selection replaces the previous one, and an invalid later selection is never
silently replaced by an earlier valid value. The built configuration is
immutable.

<!-- concord-source: crates/concord-examples/src/runtime_policy/production_http.rs#production_client -->
```rust
pub fn build_client(
    proxy_origin: &str,
    root_certificate_pem: &[u8],
) -> Result<ProductionApi, Box<dyn std::error::Error>> {
    let proxy = SafeProxy::parse(proxy_origin)?;
    let root_certificate_pem = root_certificate_pem.to_vec();
    let client = ProductionApi::builder()
        .configure_http(move |http| {
            http.require_https()
                .proxy(proxy)
                .connect_timeout(Duration::from_secs(5))
                .read_timeout(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(8)
                .tcp_keepalive(Duration::from_secs(60))
                .tcp_nodelay(true)
                .add_root_certificate_pem(root_certificate_pem)
        })
        .configure_runtime(|runtime| {
            runtime
                .runtime_endpoint_timeout(Duration::from_secs(30))
                .request_body_limit(BODY_LIMIT)
                .response_body_limit(BODY_LIMIT)
                .pagination_total_response_body_limit(TRAVERSAL_BODY_LIMIT)
                .stream_response_limit(64 * 1024 * 1024)
        })
        .build()?;
    Ok(client)
}
```

## `HttpConfigBuilder`

| Method | Effect |
|---|---|
| `require_https()` | a positive restriction: the managed client refuses non-HTTPS requests |
| `allow_http()` | explicitly permits HTTP or HTTPS (the default) |
| `proxy(SafeProxy)` | routes connections through a validated proxy; the logical origin is unchanged |
| `add_root_certificate_pem(pem)` / `identity_pem(pem)` | trust roots and client identity (bounded PEM input) |
| `connect_timeout(d)`, `read_timeout(d)` | transport timeouts |
| `pool_idle_timeout(d)`, `pool_max_idle_per_host(n)` | connection-pool choices |
| `tcp_keepalive(d)`, `tcp_nodelay(bool)` | socket options |
| `http1_only()`, `http2_prior_knowledge()` | protocol selection (HTTP/2 needs the `http2` feature) |
| `protocol_recovery(bool)` | enables or disables transport-private recovery of proven-unprocessed tries |
| `provider_retry(ProviderRetry)` | the provider-helper HTTP retry choice |
| `disable_gzip()`, `disable_brotli()`, `disable_deflate()` | turn off an enabled decompression feature |

Automatic redirects, cookie stores and native upgrades are disabled in every
configuration. Credential-bearing requests always require HTTPS and TLS,
whatever `allow_http()` says.

## `RuntimeConfigBuilder`

Count and size setters take primitive values (§20.4); zero is a build error
except where a disable method exists.

| Method | Argument | Default |
|---|---|---|
| `runtime_endpoint_timeout` | positive `Duration` | 30 s, for runtime descriptions without an owner selection |
| `request_body_limit` | positive `usize` bytes | 16 MiB |
| `response_body_limit` | positive `usize` bytes | 16 MiB (buffered accepted responses) |
| `stream_response_limit` | positive `usize` bytes | 16 MiB |
| `status_body_limit` / `disable_status_body_capture` | positive `usize` / none | 64 KiB |
| `provider_request_limit`, `provider_response_limit` | positive `usize` bytes | 1 MiB each |
| `provider_max_sends` | positive `u32`, at most 64 | 4 per acquisition |
| `provider_max_origins` | positive `usize`, at most 32 | 8 |
| `provider_timeout` | positive `Duration` | 30 s (under the remaining operation deadline) |
| `pagination_max_pages` | positive `u32` | 1,000 |
| `pagination_max_items` | positive `usize` | 100,000 |
| `pagination_total_response_body_limit` | positive `usize` bytes | 64 MiB |
| `pagination_identity_limit` | positive `usize` bytes | 4 MiB |
| `aggregate_max_sends` | positive `u32`; intersected with per-call `cap_sends` | 2,000 per aggregate |
| `stream_lifetime`, `stream_idle_timeout` | positive `Duration` | 5 min, 30 s |
| `stream_max_records`, `stream_frame_limit` | positive `usize` | 100,000, 1 MiB + 2 bytes |
| `auth_failure_cooldown_cap`, `rate_limit_cooldown_cap` | finite `Duration`; zero disables retaining that cooldown | 60 s each |
| `rate_limiter` | `Arc<dyn RateLimiter>` | none: required when any rate policy is reachable |
| `retry_policy` / `disable_retry_policy` | `RetryPolicy` / none | no retry |
| `operation_observer` | `Arc<dyn OperationObserver>` | none |
| `debug` | `DebugLevel` (`None`, `Basic`, `Headers`) | `None` |

The aggregate handoff ceiling (`aggregate_max_sends`, 2,000 by default) counts
every application handoff of one traversal and is intersected with a request's
`cap_sends`, which can only narrow it; each direct request or page keeps its own
18-attempt ceiling.

`runtime_endpoint_timeout` never rewrites generated declarations: the root
default is 30 seconds or the declared `timeout`. At runtime binding the owner's
effective timeout applies unless the runtime endpoint selects its own.

## Build-time validation

`build()` may parse static configuration and construct transport and pool
resources. It never performs network access, provider acquisition or remote
schema registration. It reports, together: missing required values, secrets
and providers; invalid HTTP or runtime settings; a missing limiter for a
client with a reachable rate policy; a limiter whose key capability cannot
admit the declared keys; and an `https` base without an enabled TLS feature
(`TlsUnavailable`).

## Centralized limits

Constants and runtime fields have one owner in core; generated metadata, docs,
plans and tests read them (LIMIT-01). Reaching a limit is a structured error,
never wraparound, hidden truncation or an unbounded fallback.

| Resource | Default / hard constraint |
|---|---|
| Operation timeout | 30 s when omitted; finite |
| Request body / buffered response / stream bytes | 16 MiB each, configurable |
| Status body capture | 64 KiB, configurable, explicit disable |
| Provider request/response body | 1 MiB each |
| Provider helper sends | 4 per acquisition; hard maximum 64 |
| Provider allowed origins | 8; hard maximum 32 |
| Traversal pages / items / acquired bytes / identity | 1,000 / 100,000 / 64 MiB / 4 MiB |
| Application handoffs | 2,000 per aggregate (`aggregate_max_sends`); 18 per direct request or page (initial + 16 policy + 1 auth) |
| Committed quota admissions | at most three per allowed handoff (initial + two freshness restarts) |
| Policy retries | off by default; installed default 2, hard 16 per direct request or page |
| Auth recovery replays | at most 1 per direct request or page |
| Transport-private tries | at most 2 per visible attempt |
| Stream lease / idle / records / frame | 5 min / 30 s / 100,000 / 1 MiB + 2 bytes |
| Empty chunks | 1,024 consecutive; at most 32 per poll before yielding |
| Formatted public atom | 65,536 bytes |
| Logical and sanitized URL | 131,072 bytes each |
| Public head components | 256 |
| Header count / aggregate bytes | 256 / 65,536 |
| Static query/header name | 256 bytes |
| Runtime recorded operations / retained bytes | 256 / 65,536 |
| Method token | 128 bytes |
| Effective auth placements | 64 |
| `SecretString` | 1 MiB |
| Single credential / Basic user / password | 65,536 / 8,192 / 32,768 bytes |
| Policy ID / custom key label | 256 / 128 bytes |
| Rate dimensions / windows / policies | 32 / 16 / 64 per effective plan |
| Resolved rate key | 1,024 default; hard 65,536; intersected with the limiter |
| Local quota / cooldown entries | 4,096 each; 15-minute fully-replenished idle TTL |
| Active schema registrations | 4,096 per local/mock shared service |
| Auth failure / rate feedback cooldown | 60 s cap each |
| Controller header interests | 16; 64 KiB retained evidence |
| Cancellation sources | 8 per request |
| Endpoint ID | 128 ASCII bytes; 63 per segment |

A byte is an octet; MiB is 1,048,576 bytes. The public constants are exported
from the role modules (for example `concord::endpoint::MAX_REQUEST_URL_BYTES`,
`concord::auth::MAX_AUTH_PLACEMENTS`, `concord::rate_limit::MAX_RATE_LIMIT_KEY_BYTES`).

## What limits do not bound

Wire and resource ceilings do not bound memory already owned by a caller's
values or allocation performed by a trusted decoder: atom limits apply to
encoded output, not to arbitrary object graphs. Deadlines are checked around
synchronous callbacks but cannot preempt them. Native TCP/TLS stacks keep their
own bounded buffers. Concord provides no plugin sandbox, per-object resource
tracker or hidden task scheduler; extensions are trusted code with documented
obligations (LIMIT-02).

## Features

Features add integrations without changing the meaning of an enabled
declaration; a representation whose feature is missing is a compile error, not
a fallback. The facade's defaults are `default-tls`, `http2` and `rate-limit`;
`json`, `oauth2-client`, `records`, `csv`, `ndjson`, `multipart`, `gzip`,
`brotli` and `deflate` are opt-in. The [README](../README.md#cargo-features)
feature matrix is checked against the facade manifest. The core runtime needs
no JSON dependency to execute a declaration; the `concord.contract/3`
serialization lives in the `concord-inspect` companion crate.
