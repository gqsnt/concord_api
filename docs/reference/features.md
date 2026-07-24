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
| `default` | meta | Default TLS, HTTP/2, and process-local governor |
| `default-tls` | yes | HTTPS with the ordinary default TLS backend |
| `http2` | yes | HTTP/2 transport support |
| `rate-limit-governor` | yes | Process-local generated rate-plan enforcement |
| `json` | no | `Json<T>` request and response codec |
| `oauth2-client` | no | OAuth2 client-credentials provider helper |
| `records` | no | Generic bounded record streaming |
| `csv` | no | CSV record decoder (also enables `records`) |
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
  governor.
- JSON remains opt-in. CSV and NDJSON imply generic record support.
- Built-in pagination controllers require no ecosystem feature.
- Multipart remains a runtime-kernel capability because native construction
  and replayability enforcement cannot cross the extension boundary.
- OAuth2 client credentials, JSON, CSV, NDJSON, and governor ecosystem
  dependencies are enabled only by their named façade features.
- Dangerous features never join the default set.

Use `default-features = false` for a minimal HTTP-capable contract and add only
the listed capabilities:

```toml
concord = {
    version = "0.1.0",
    default-features = false,
    features = ["json", "default-tls"]
}
```
