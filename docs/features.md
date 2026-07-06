# Concord Feature Matrix

Concord keeps feature surfaces explicit and minimal. This document records the supported crate defaults, optional feature ownership, and the matrix enforced by `scripts/check_features.sh`.

## Matrix

| Crate | Default features | Optional features | Supported no-default build | Notes |
| --- | --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart`, `rate-limit-governor` | yes | `json` enables `reqwest/json`. `serde`, `serde_json`, and `reqwest` are always present in `concord_core`; `reqwest` carries the selected transport features used by the runtime. When `rate-limit-governor` is off, the default limiter fails closed for non-empty declared plans and `NoopRateLimiter` is the explicit opt-out. |
| `concord_macros` | none | none | yes | Proc-macro crate. |
| `concord_examples` | none | none | no | Compile-checked examples depend on `concord_core` with `json` enabled. |

## Compile / Check Matrix

The following commands are intentionally supported and are checked by `scripts/check_features.sh`:

```bash
cargo check -p concord_core --no-default-features
cargo check -p concord_core --no-default-features --features json
cargo check -p concord_core --all-features
cargo test -p concord_core --no-default-features no_default_rate_limit
cargo test -p concord_core --no-default-features --features json no_default_rate_limit

cargo check -p concord_macros
cargo check -p concord_macros --all-features

cargo check -p concord_examples --all-targets
cargo check -p concord_examples --all-targets --all-features
```

`concord_examples --no-default-features` is intentionally unsupported.

`scripts/check_features.sh` also checks dependency-tree invariants:

- the `concord_core` default tree contains `rate-limit-governor` and omits optional HTTP codecs that are not enabled by default;
- the `concord_core --no-default-features` tree omits the default `governor` feature edge;
- the `concord_macros` default and `--no-default-features` trees are identical and omit runtime-only crates such as `serde_json`.

## Dependency Ownership

- `json` toggles the JSON-specific `reqwest/json` integration in `concord_core`.
- `serde`, `serde_json`, and `reqwest` remain unconditional `concord_core` dependencies.
- `concord_macros` must not widen the runtime feature surface through its normal dependency tree.
- `concord_examples` may enable richer core features because it is a compile-checked example crate.

## Feature-Relevant Nextest Runtime Matrix

The feature-relevant runtime commands in the local gate currently run:

```bash
cargo nextest run -p concord_core
cargo nextest run -p concord_core --all-features
cargo nextest run -p concord_examples
cargo nextest run -p concord_examples --all-features
cargo nextest run --workspace
cargo nextest run --workspace --all-features
cargo nextest run --workspace --all-targets
```

Feature-flavored core nextest invocations are intentionally omitted for now when they rely on the default-disabled core runtime path. The current core runtime suite is not feature-parametric, and these commands fail in the rate-limit characterization tests:

```bash
cargo nextest run -p concord_core --no-default-features
cargo nextest run -p concord_core --no-default-features --features json
```

`scripts/check_features.sh` remains the compile/check feature matrix. `scripts/check_v1.sh` owns the full local gate.

## Extending The Surface

When adding a new optional feature:

1. Add the feature explicitly in the owning crate.
2. Keep it out of `default` unless a deliberate release decision says otherwise.
3. Add a feature-matrix check in `scripts/check_features.sh`.
4. Update this table and any example fixtures that intentionally consume the new feature.
