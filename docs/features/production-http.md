# Production HTTP

Configure transport and runtime policy on the generated client builder before
sharing the built client. Concord starts from conservative transport behavior:
redirects and environment proxy discovery are disabled, only private
known-safe protocol recovery is enabled, and provider HTTP remains isolated.

## Build an explicit deployment configuration

This compiled example requires `default-tls`, restricts application traffic to
HTTPS, installs an explicit proxy and private trust root, sets transport
timeouts and pool policy, and chooses finite body limits:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/runtime_policy/production_http.rs:production_client}}
```

The endpoint declaration still owns its generated `timeout 10s`. The
`runtime_endpoint_timeout` setting applies only to runtime-defined endpoints
that inherit the client default. Request-local timeout controls can replace or
disable the selected endpoint timeout.

Use `configure_http` for native connection behavior and
`configure_runtime` for Concord lifecycle limits, hooks, and limiter policy.
Build a new client snapshot when those deployment choices change.

## TLS and protocol selection

Enable `default-tls` for ordinary HTTPS with Concord's reviewed TLS backend.
`https_only(true)` rejects non-HTTPS application URLs at transport use; the
OAuth2 helper and provider HTTP already require HTTPS.

`add_root_certificate_pem` installs a structurally bounded trust root for both
application and provider clients. Parsing occurs during `build()`, so malformed
PEM fails before requests can run. `identity_pem` installs a bounded client
identity only on the application client. Provider HTTP deliberately does not
present the application identity.

Negotiated protocol selection is the default. `http1_only()` restricts both
managed clients to HTTP/1. With the `http2` feature,
`http2_prior_knowledge()` selects prior-knowledge HTTP/2 where the deployment
requires it.

## Redirects and proxies

Concord disables automatic redirects for application and provider traffic.
A received 3xx remains a final HTTP status, so request shape, credential
placement, rate admission, and hooks cannot be silently reused at a different
origin.

Environment proxy discovery is also disabled. Parse one reviewed proxy origin
with `SafeProxy::parse` and install it explicitly with `.proxy(...)`. The
explicit proxy applies to application and provider HTTP. Keep proxy
credentials and configuration under application control; safe plans describe
the logical request rather than the proxy exchange.

## Compression and byte limits

The optional `gzip`, `brotli`, and `deflate` features enable the corresponding
response decompression. Their `disable_*` builder methods let a deployment turn
an enabled algorithm off.

Response, stream, status-body, and provider-response limits count bytes after
supported transport decompression. A small compressed payload cannot expand
past the configured managed limit. Application values produced by a decoder
still own their own allocation and CPU bounds.

## Timeouts, pools, and protocol recovery

Native connection and per-read timeouts complement the Concord operation
timeout; they do not replace it. Pool idle timeout and maximum idle
connections per host bound reuse policy. TCP keepalive and no-delay are
explicit deployment choices.

Private known-safe protocol recovery is enabled by default. It remains inside
one visible Concord attempt, performs no status retry, does not invoke the
managed retry classifier, and cannot resend a non-cloneable native body.
`.protocol_recovery(false)` disables it. Provider recovery uses the separate
reviewed `ProviderRetry` selection.

Application retry is the distinct Concord-managed `RetryPolicy` surface. It is
off by default and is documented under [Managed retries](retries.md).

## Deployment checklist

Before releasing a client configuration, verify:

- the fixed base and `https_only` policy match every generated and runtime
  endpoint;
- required public roots and optional client identity parse during startup;
- the explicit proxy is intended for both application and provider traffic;
- connect, read, endpoint, and consumer-stream deadlines cover different
  liveness phases;
- body, stream, status, pagination, provider, limiter, and hook bounds match
  expected payload and cardinality;
- managed retry is installed only for operations whose remote semantics permit
  repeated sends;
- logging treats sanitized public URLs and headers as potentially sensitive
  application data.

Concord intentionally does not expose the Reqwest client or builder through
ordinary configuration. If a deployment requirement cannot be expressed by
the reviewed surface, treat it as an extension/design decision rather than
reaching around the managed lifecycle.

## Related

- [Build and share clients](../guide/client-construction.md)
- [Hooks and observability](hooks-and-observability.md)
- [Feature flags](../reference/features.md)
- [Limits reference](../reference/limits.md)
- [Managed retries](retries.md)
