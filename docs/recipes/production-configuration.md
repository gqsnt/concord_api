# Production configuration

## Goal

Build one immutable HTTPS client with an explicit proxy, private trust root,
transport liveness settings, and evidence-based finite body limits—without
enabling broad native access or unsafe retry defaults.

## Complete code

The complete compiled recipe declares a generated endpoint and configures the
deployment builder:

```rust,ignore
{{#include ../../crates/concord-examples/src/runtime_policy/production_http.rs:production_recipe}}
```

The caller supplies a reviewed proxy origin and PEM trust root. `build()`
parses the TLS material and fails before any request can execute when transport
configuration is invalid. `ClientBuildErrorKind` distinguishes
`InvalidRootCertificate`, `InvalidClientIdentity`, `InvalidProxy`,
`HttpsRequired`, `TlsUnavailable`, and `HttpClientBuild` so startup handling
can report the remediation category without logging supplied material or a
native error.

## Why these declarations

`ProductionApi` keeps the service contract small: a fixed HTTPS base and a
10-second endpoint timeout. Transport choices stay outside the declaration.

`https_only(true)` rejects non-HTTPS application transport. `SafeProxy::parse`
makes proxy use explicit; Concord ignores environment proxy variables. The
proxy and custom root apply to both application and isolated provider HTTP.

Connect and read timeouts bound native phases. Pool idle settings and TCP
options describe connection reuse rather than endpoint semantics.

`runtime_endpoint_timeout` supplies the default only for runtime-defined
endpoints that inherit it. It does not replace the generated health endpoint's
declared timeout.

Separate request, buffered-response, and stream-response limits match their
different retention modes. Positive `NonZeroUsize` inputs cannot smuggle an
unbounded zero sentinel into runtime configuration.

## Adaptations

Add `.identity_pem(...)` when the application service requires client TLS.
That identity is installed only on the application client; provider HTTP does
not inherit it. Use a custom provider design when a token endpoint has a
separate identity requirement.

Enable `gzip`, `brotli`, or `deflate` only when needed. Corresponding
`disable_*` methods can turn an enabled algorithm off for one client.
Managed response limits count post-decompression bytes.

Install bounded hooks with `configure_runtime(...hooks(...))` and choose a
limiter appropriate to local or distributed quota ownership. Clones of the
built client share both objects. Independently built clients are isolated
unless given the same `Arc`.

Add status-body, provider-body, pagination, authentication-cooldown, and
rate-cooldown settings when defaults do not match measured workloads. Keep
exact values in application configuration rather than duplicating Concord's
default table.

Known-safe protocol recovery is enabled by default and remains inside one
visible Concord attempt. Use `protocol_recovery(false)` only when the
deployment requires exactly one physical send. Install application retry
separately through an explicit managed `RetryPolicy`.

## Production notes

At startup, verify the fixed base, proxy, trust roots, optional identity, TLS
feature set, and every required protected input. Treat `ClientBuildError` as a
deployment failure, not a per-request retry. Match `error.kind()` with a
fallback rather than parsing its formatting; the non-exhaustive kind is the
stable diagnostic surface and `error.source()` is intentionally absent.

Redirects remain disabled. A 3xx response is a typed final status rather than
an automatic cross-origin request. Keep that property in security and
availability tests.

Choose limits and timeouts from payload distributions and service objectives.
Endpoint execution, later stream polling, completion work, and provider HTTP
have distinct liveness boundaries. Add application-owned timeouts to trusted
callbacks and stream consumers where needed.

Log stable endpoint and operation IDs, outcome categories, bounded timing, and
approved labels. Sanitized public URLs and headers can still contain business
or personal identifiers. Avoid dynamic high-cardinality metric keys.

Rebuild when stable configuration changes and swap the client through the
application's lifecycle. Do not mutate a built client's transport in place.

## Related

- [Production HTTP](../features/production-http.md)
- [Build and share clients](../guide/client-construction.md)
- [Hooks and observability](../features/hooks-and-observability.md)
- [Limits reference](../reference/limits.md)
- [Managed retries](retries.md)
