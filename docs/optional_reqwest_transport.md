# Reqwest Foundation Alignment (Slice Notes)

`concord_core` now treats reqwest as the mandatory transport foundation. The optional
feature slice is complete, and the temporary architecture remains unchanged:

- `ApiClient` keeps transport polymorphism (`with_transport(...)` remains available).
- `ReqwestTransport` remains the default transport type.
- Generated clients, `ApiClient<Cx, T>`, and request/response transport boundaries are unchanged.
- Managed configuration remains available through `with_reqwest_builder(...)`,
  whose closure receives `SafeReqwestBuilder`, not a raw Reqwest builder.
- Retry mode remains `reqwest::retry::never()` during this migration slice.

## Dependency policy

- Workspace `Cargo.toml` sets `reqwest = { version = "=0.13.4", default-features = false }`.
- `concord_core/Cargo.toml` uses `reqwest` unconditionally as a dependency.
- `perf/Cargo.toml` uses an independent but equivalent audited policy with `reqwest = { version = "=0.13.4", default-features = false }` so its excluded lockfile resolves the same audited patch.

## What changed for the feature story

- `transport-reqwest` feature and reqwest-optional branches were removed.
- `--no-default-features` remains a valid build target and still includes a reqwest-backed runtime with streaming transport support.
- Default-enabled transport capabilities:
  - `default-tls`
  - `http2`
- Opt-in transport capabilities:
  - `gzip`
  - `brotli`
  - `deflate`
  - `multipart`
- No-default transport capabilities:
  - `stream` only

No-default builds do not implicitly enable system-proxy, governor, dev-seam features, dangerous raw response,
or additional compression/cookie/multipart features.

## Validation expectation for this slice

- `cargo check -p concord_core --no-default-features` must succeed and execute a reqwest-backed transport.
- `reqwest` is mandatory in both `concord_core` and `perf` dependency graphs at version `0.13.4`.
- No source claims or tests should describe a reqwest-free runtime as current.
