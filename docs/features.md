# Concord Feature Matrix

Concord keeps feature surfaces explicit and minimal. This document records the supported crate defaults and optional feature ownership. `just release` validates the combined all-feature workspace configuration.

See [Security Model](security_model.md) for the consumer-facing boundary between safe, advanced, and dangerous surfaces.

## Matrix

| Crate | Default features | Optional features | Supported no-default build | Notes |
| --- | --- | --- | --- | --- |
| `concord_core` | `default-tls`, `http2`, `rate-limit-governor` | `json`, `default-tls`, `http2`, `gzip`, `brotli`, `deflate`, `multipart`, `dangerous-raw-response`, `dangerous-dev-tools` | yes | Reqwest `=0.13.4` is mandatory in every build. `new()` and `builder()` always create the managed Reqwest client, including with `--no-default-features`. Optional features add reviewed Reqwest capabilities; cookies and redirects remain unavailable. Dangerous and development surfaces require explicit features. When `rate-limit-governor` is off, non-empty declared plans fail closed and `NoopRateLimiter` is the explicit opt-out. |
| `concord_macros` | none | none | yes | Proc-macro crate. |
| `concord_examples` | none | `dangerous-raw-response`, `dangerous-dev-tools` | no | Compile-checked examples depend on `concord_core` with `json` enabled and forward the dangerous escape-hatch features for example-specific compile checks; neither feature is enabled by default. |

## Compile / Check Matrix

The following focused commands are useful when diagnosing feature support:

```bash
cargo check -p concord_core --no-default-features
cargo check -p concord_core --no-default-features --features json
cargo check -p concord_core --no-default-features --features dangerous-dev-tools
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

- the `concord_core` default tree contains `default-tls`, `http2`, and `rate-limit-governor`;
- `concord_core --no-default-features` keeps a reqwest-backed transport with `stream` and omits optional HTTP transport capabilities (`default-tls`, `http2`, `gzip`, `brotli`, `deflate`, and `multipart`); HTTPS is rejected before execution in this mode.
- the `concord_core --no-default-features` tree omits the default `governor` feature edge;
- the `concord_macros` default and `--no-default-features` trees are identical and omit runtime-only crates such as `serde_json`.

## Dependency Ownership

- `json` keeps the built-in JSON and OAuth2 auth helpers available in `concord_core` without adding reqwest transport capabilities like gzip/brotli/deflate.
- `reqwest` is mandatory in `concord_core` and is the default transport dependency.
- `default-tls`, `http2`, `gzip`, `brotli`, `deflate`, and `multipart` enable optional reqwest transport capabilities. `default-tls` and `http2` are enabled in default builds via default features.
- `serde` and `serde_json` remain unconditional `concord_core` dependencies.
- `concord_macros` must not widen the runtime feature surface through its normal dependency tree.
- `concord_examples` may enable richer core features because it is a compile-checked example crate.

## Focused Feature Diagnostics

No-default and all-feature core checks are composed into `just release`.
Additional feature-specific Nextest runs remain useful diagnostics:

```bash
cargo nextest run -p concord_core --no-default-features
cargo nextest run -p concord_core --no-default-features --features json
```

The root `justfile` owns the canonical compile, test, documentation,
supply-chain, performance-package, and benchmark-compilation gate.

## Explicit Development Seam

`concord_core::__development` is an unstable deterministic-test observation
seam. It is compiled only when `dangerous-dev-tools` is explicitly selected;
ordinary downstream debug builds do not expose it, and the feature is not in
`concord_core`'s defaults. The module exposes purpose-specific lifecycle
observations and opaque snapshots, not credential slots, request/response
planning types, body engines, or request execution errors. Generated clients and
normal examples never import it. Snapshot identities support cloning and
equality comparison only; their constructors, numeric representation, and
ordering are private, and their diagnostics render only an opaque label.

The feature provides only the narrow lifecycle observation seam used by
deterministic tests. It does not expose body persistence or runtime planning
types and is not enabled by default.

## Extending The Surface

When adding a new optional feature:

1. Add the feature explicitly in the owning crate.
2. Keep it out of `default` unless a deliberate release decision says otherwise.
3. Update this table and any example fixtures that intentionally consume the new feature.
