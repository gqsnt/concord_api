# Concord Feature Matrix

Concord keeps feature surfaces explicit and minimal. This document records the supported crate defaults, optional feature ownership, and the matrix enforced by `scripts/check_features.sh`.

## Matrix

| Crate | Default features | Optional features | Supported no-default build | Notes |
| --- | --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart` | yes | `json` owns `serde`, `serde_json`, and `reqwest/json`. |
| `concord_macros` | none | none | yes | Proc-macro crate. |
| `concord_examples` | none | none | no | Compile-checked examples depend on `concord_core` with `json` enabled. |

## Supported Commands

The following commands are intentionally supported and are checked by `scripts/check_features.sh`:

```bash
cargo check -p concord_core --no-default-features
cargo check -p concord_core --no-default-features --features json
cargo check -p concord_core --all-features

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

- `json` is owned by `concord_core`.
- `concord_macros` must not widen the runtime feature surface through its normal dependency tree.
- `concord_examples` may enable richer core features because it is a compile-checked example crate.

## Extending The Surface

When adding a new optional feature:

1. Add the feature explicitly in the owning crate.
2. Keep it out of `default` unless a deliberate release decision says otherwise.
3. Add a feature-matrix check in `scripts/check_features.sh`.
4. Update this table and any example fixtures that intentionally consume the new feature.
