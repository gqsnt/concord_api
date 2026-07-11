# Concord Feature Matrix

Concord keeps feature surfaces explicit and minimal. This document records the supported crate defaults and optional feature ownership. `just release` validates the combined all-feature workspace configuration.

See [Security Model](security_model.md) for the consumer-facing boundary between safe, advanced, and dangerous surfaces.

## Matrix

| Crate | Default features | Optional features | Supported no-default build | Notes |
| --- | --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor`, `transport-reqwest` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart`, `rate-limit-governor`, `transport-reqwest`, `dangerous-raw-response`, `dangerous-dev-tools` | yes | `json` keeps the built-in JSON/auth helpers available without reqwest. `transport-reqwest` owns `ReqwestTransport` and the reqwest transport codec features. `dangerous-raw-response` enables the raw-response escape hatch and `dangerous-dev-tools` enables the dev-body-capture configuration API; neither feature enables the escape hatch by itself. `serde` and `serde_json` are always present in `concord_core`; `reqwest` is optional and only enters the build when `transport-reqwest` or a reqwest codec feature is enabled. When `rate-limit-governor` is off, the default limiter fails closed for non-empty declared plans and `NoopRateLimiter` is the explicit opt-out. |
| `concord_macros` | none | none | yes | Proc-macro crate. |
| `concord_examples` | none | `dangerous-raw-response`, `dangerous-dev-tools` | no | Compile-checked examples depend on `concord_core` with `json` enabled and forward the dangerous escape-hatch features for example-specific compile checks; neither feature is enabled by default. |

## Compile / Check Matrix

The following focused commands are useful when diagnosing feature support:

```bash
cargo check -p concord_core --no-default-features
cargo check -p concord_core --no-default-features --features json
cargo check -p concord_core --no-default-features --features transport-reqwest
cargo check -p concord_core --no-default-features --features dangerous-dev-tools
cargo check -p concord_core --no-default-features --features "transport-reqwest json"
cargo check -p concord_core --all-features
cargo test -p concord_core --no-default-features no_default_rate_limit
cargo test -p concord_core --no-default-features --features json no_default_rate_limit

cargo check -p concord_macros
cargo check -p concord_macros --all-features

cargo check -p concord_examples --all-targets
cargo check -p concord_examples --all-targets --features dangerous-dev-tools
cargo check -p concord_examples --all-targets --all-features
```

`concord_examples --no-default-features` is intentionally unsupported.

The dependency-tree invariants are documented here for focused diagnosis when
feature ownership changes:

- the `concord_core` default tree contains `rate-limit-governor` and omits optional HTTP codecs that are not enabled by default;
- the `concord_core --no-default-features` tree omits the default `governor` feature edge;
- the `concord_macros` default and `--no-default-features` trees are identical and omit runtime-only crates such as `serde_json`.

## Dependency Ownership

- `json` keeps the built-in JSON and OAuth2 auth helpers available in `concord_core` without pulling in reqwest.
- `transport-reqwest` owns the reqwest-backed transport and the reqwest codec features.
- `serde` and `serde_json` remain unconditional `concord_core` dependencies.
- `reqwest` is optional and only appears when `transport-reqwest` or one of the reqwest codec feature flags is enabled.
- `concord_macros` must not widen the runtime feature surface through its normal dependency tree.
- `concord_examples` may enable richer core features because it is a compile-checked example crate.

## Focused Feature Diagnostics

No-default and feature-specific commands remain optional diagnostics. They are
not dependencies of `just release`:

```bash
cargo nextest run -p concord_core --no-default-features
cargo nextest run -p concord_core --no-default-features --features json
```

The root `justfile` owns the canonical compile, test, documentation, and
supply-chain gate. It does not claim individual feature isolation or
dependency-tree equivalence.

## Extending The Surface

When adding a new optional feature:

1. Add the feature explicitly in the owning crate.
2. Keep it out of `default` unless a deliberate release decision says otherwise.
3. Update this table and any example fixtures that intentionally consume the new feature.
