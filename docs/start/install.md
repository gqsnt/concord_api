# Install Concord

Add the `concord` façade to the application that will own the generated
client. Applications should not depend directly on `concord-core`,
`concord-extras`, or `concord-macros`.

The packages are not currently published. From a checkout, use the same path
form as the repository's package graph, adjusted for your workspace:

```toml
[dependencies]
concord = { path = "../concord/crates/concord" }
```

After a registry release is explicitly approved, the intended dependency form
is:

```toml
[dependencies]
concord = "0.1.0"
```

Do not copy the version form into a build today and assume a registry package
exists. The [compatibility reference](../reference/compatibility.md) records
the current unpublished status and MSRV.

## Default features

The default façade bundle enables:

- `default-tls` for ordinary HTTPS support;
- `http2` for HTTP/2 transport;
- `rate-limit` for a bounded process-local `LocalRateLimiter`.

JSON is intentionally opt-in. A typical JSON client from a checkout uses:

```toml
[dependencies]
concord = {
    path = "../concord/crates/concord",
    features = ["json"]
}
serde = { version = "1", features = ["derive"] }
```

For a minimal feature selection, disable defaults and add only the
capabilities the contract needs:

```toml
[dependencies]
concord = {
    path = "../concord/crates/concord",
    default-features = false,
    features = ["default-tls", "json"]
}
```

Disabling defaults does not disable Concord's core lifecycle, redaction, or
finite request/response boundaries. It removes the selected TLS, HTTP/2, and
first-party local-limiter implementations. A declaration with rate-limit
rules must still receive a limiter; without a feature-provided or explicitly
configured limiter, client construction fails closed.

Use the [feature reference](../reference/features.md) for the exact list and
feature implications. CSV and NDJSON enable generic buffered and streaming
record support; both formats are response-only. Managed application retry
remains an explicit runtime policy rather than a Cargo feature.

## Async runtime ownership

Generated request terminals are Rust futures. Concord does not start an async
runtime for the application. Use the runtime already owned by your program.
For example, a Tokio binary commonly adds:

```toml
[dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Then call the client from `#[tokio::main] async fn main()`. Libraries can
return or await the generated future without choosing a global executable
runtime for their callers.

## Toolchain and TLS

The minimum supported Rust version is Rust 1.86.0. The repository's current
stable toolchain may be newer, but public product libraries are checked
separately with Rust 1.86.0.

With ordinary defaults, HTTPS uses the configured default TLS backend.
Redirects and environment proxies are not silently enabled. Custom roots,
application identity, and explicit proxy configuration are runtime-builder
choices and have structural limits. Provider HTTP inherits proxy and root
certificate configuration but not the application's client identity.

For production, keep TLS enabled unless the deployment has a deliberate
private transport boundary. The minimal Start example uses an HTTP `.test`
host solely so its deterministic loopback execution requires no certificate
setup.

## Check the dependency

Run:

```text
cargo check
```

If a declared representation or provider requires a disabled feature, Rust
reports the missing trait implementation at that declared type. Enable the
named façade feature rather than importing an implementation crate directly.

## Related

- [Your first API](first-api.md)
- [Feature flags](../reference/features.md)
- [Compatibility and MSRV](../reference/compatibility.md)
- [Production configuration](../recipes/production-configuration.md)
