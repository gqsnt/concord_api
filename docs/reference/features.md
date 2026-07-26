# Feature reference

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
| `rate-limit` | yes | Process-local generated rate-plan enforcement |
| `json` | no | `Json<T>` request and response codec |
| `oauth2-client` | no | OAuth2 client-credentials provider helper |
| `records` | no | Generic bounded record streaming |
| `csv` | no | Type-configurable CSV record decoder (also enables `records`) |
| `ndjson` | no | NDJSON record decoder (also enables `records`) |
| `multipart` | no | Managed multipart request construction |
| `gzip` | no | Gzip response decompression |
| `brotli` | no | Brotli response decompression |
| `deflate` | no | Deflate response decompression |
| `dangerous-raw-response` | no | Bounded undecoded successful-response terminal |
| `dangerous-custom-retry` | no | Custom native transport retry policy |
<!-- END GENERATED FEATURES -->

## Capability notes

- Ordinary defaults preserve default TLS, HTTP/2, and the process-local
  local limiter.
- JSON remains opt-in. Its request serializer writes through the active body
  cap, and its failures use the same finite codec categories as downstream
  codecs. CSV and NDJSON imply generic record support.
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
- Dangerous features never join the default set.
- The façade passes dangerous capability tokens to generated code; proc-macro
  crate features are not used to infer a downstream application's feature
  selection.
- Feature selection changes available capabilities; it never disables the
  core request-head, lifecycle, redaction, or acquisition ceilings.

Use `default-features = false` for a minimal HTTP-capable contract and add only
the listed capabilities:

```toml
concord = {
    version = "0.1.0",
    default-features = false,
    features = ["json", "default-tls"]
}
```
