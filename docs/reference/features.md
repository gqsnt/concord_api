# Feature flags

Applications select capabilities only on the `concord` dependency. The façade
forwards lifecycle features and composes enabled downstream implementations.
The runtime kernel and downstream implementation package both have empty
defaults.

The table is checked against `crates/concord/Cargo.toml` by the documentation
task.

<!-- BEGIN GENERATED FEATURES -->
| Feature | Enabled by `default` | Capability |
|---|---:|---|
| `default` | meta | Default TLS, HTTP/2, and process-local rate limiting |
| `default-tls` | yes | HTTPS with the ordinary default TLS backend |
| `http2` | yes | HTTP/2 transport support |
| `rate-limit` | yes | Bounded process-local `LocalRateLimiter` policy |
| `json` | no | `Json<T>` request and response codec |
| `oauth2-client` | no | OAuth2 client-credentials provider helper |
| `records` | no | Generic bounded buffered and streaming record delivery |
| `csv` | no | Type-configurable CSV buffered/streaming response decoder (also enables `records`) |
| `ndjson` | no | NDJSON buffered/streaming response decoder (also enables `records`) |
| `multipart` | no | Managed multipart request construction |
| `gzip` | no | Gzip response decompression |
| `brotli` | no | Brotli response decompression |
| `deflate` | no | Deflate response decompression |
<!-- END GENERATED FEATURES -->

## Select a feature set

The ordinary dependency uses the default bundle:

```toml
[dependencies]
concord = "0.1.0"
```

For an explicit minimal set, disable defaults and list every required
capability:

```toml
[dependencies.concord]
version = "0.1.0"
default-features = false
features = ["json", "default-tls"]
```

With no default features, cleartext HTTP/1 contracts and the core managed
lifecycle remain available. HTTPS construction requires `default-tls`;
HTTP/2 requires `http2`; and a declaration with nonempty rate rules requires
an explicitly configured limiter when `rate-limit` is absent.

Cargo features are additive and unified for each resolved package. Enabling a
feature through any dependency enables its public surface for the whole
binary. Audit the resolved feature graph rather than assuming one leaf
dependency can keep a capability private.

## Feature composition

| Selection | Implied capability |
|---|---|
| `default` | `default-tls`, `http2`, and `rate-limit` |
| `csv` | `records` plus direct buffered and streaming CSV response decoding |
| `ndjson` | `records` plus direct buffered and streaming NDJSON response decoding |
| `records` alone | generic `BufferedRecords<D>`, `Records<D>`, and downstream decoder contracts |
| `oauth2-client` | helper types only; provider HTTP still needs a TLS-capable build to call an HTTPS token endpoint |
| all decompression features | independent gzip, Brotli, and deflate response decoding |

## Capability notes

- Ordinary defaults preserve default TLS, HTTP/2, and the process-local
  `LocalRateLimiter`. The limiter is created per built client; clones share it,
  while independent builds do not share unless configured with the same
  `Arc<dyn RateLimiter>`.
- JSON remains opt-in. Its request serializer writes through the active body
  cap, and its failures use the same finite codec categories as downstream
  codecs. CSV and NDJSON imply generic record support. Direct `Csv<T>` and
  `NdJson<T>` responses collect complete vectors; wrap either in `Records<D>`
  for streaming. Neither format implements request encoding.
- `default-tls` exposes structurally bounded custom-root and application
  identity configuration. Native TLS parsing may reject bounded input more
  strictly. Proxy URL text is bounded independently of the TLS feature.
- CSV header presence and comma, semicolon, or tab parsing are selected with
  sealed type policies. Every CSV configuration retains the `text/csv` media
  contract.
- Built-in pagination controllers require no ecosystem feature.
- Multipart remains a runtime-kernel capability because native construction
  and replayability enforcement cannot cross the extension boundary. Part and
  recipe construction is fallible and structurally bounded.
- OAuth2 client credentials, JSON, CSV, NDJSON, and local rate-limit ecosystem
  dependencies are enabled only by their named façade features.
- `rate-limit` exposes `LocalRateLimiter` and `RetryAfterObserver` through
  `concord::rate_limit`. Calling `.observer(...)` replaces the built-in
  observer, so custom policy must delegate when bounded `Retry-After`
  handling should remain.
- Generated request terminals do not depend on façade feature tokens.
- Feature selection changes available capabilities; it never disables the
  core request-head, lifecycle, redaction, or acquisition ceilings.

## Verification combinations

The maintained local matrix includes the façade with no default features and
with all features, plus package-specific feature tests. An application should
also compile the exact feature set it ships; an all-features build can hide a
missing declaration in `Cargo.toml`.

Feature-disabled helpers fail as ordinary missing items or unsatisfied trait
bounds. The macro does not infer that a type called `Json`, `Csv`, or
`OAuth2ClientCredentials` should enable anything.

## Related

- [Install Concord](../start/install.md)
- [Complete DSL reference](dsl.md#feature-dependent-capabilities)
- [Limits](limits.md)
- [Compatibility and MSRV](compatibility.md)
- [Managed retries](../features/retries.md)
