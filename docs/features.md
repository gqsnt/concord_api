# Concord feature matrix

Concord keeps feature surfaces explicit and minimal. This document records the
supported crate defaults, optional feature ownership, and the matrix enforced by
`scripts/check_features.sh`.

## Matrix

| Crate | Default features | Optional features | Supported no-default build | Notes |
| --- | --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart`, `cache-moka` | yes | `json` owns `serde` / `serde_json` / `reqwest/json`. `cache-moka` owns the optional cache backend. |
| `concord_macros` | none | none | yes | Proc-macro crate. It must not default-enable runtime cache backends. Normal dependency trees stay free of cache backends; test-only dependency surfaces are separate. |
| `concord_examples` | `cache-moka` | `cache-moka` | no | Compile-checked examples intentionally use richer runtime support than the minimal crates. They depend on `concord_core` with `json` enabled and fail without `cache-moka`. |

## Supported commands

The following combinations are intentionally supported and are checked in CI or
the local gate:

```bash
cargo check -p concord_core --no-default-features
cargo check -p concord_core --no-default-features --features json
cargo check -p concord_core --no-default-features --features cache-moka
cargo check -p concord_core --no-default-features --features json,cache-moka

cargo check -p concord_macros --no-default-features
cargo check -p concord_macros

cargo check -p concord_examples --all-targets
cargo nextest run -p concord_examples
```

`concord_examples --no-default-features` is intentionally unsupported. The
fixture set includes cache-backed examples, and the macro-generated diagnostics
require `cache-moka` for those examples to compile.

## Dependency ownership

- `json` is owned by `concord_core`.
- `cache-moka` is owned by `concord_core`.
- `concord_macros` should not pick up optional runtime backends through its
  normal dependency tree.
- `concord_examples` may enable richer features than core/macros because it is
  a compile-checked example and smoke-fixture crate.

## Extending the surface

When adding a new optional backend:

1. Add the feature explicitly in the owning crate.
2. Keep it out of `default` unless a deliberate release decision says otherwise.
3. Add a feature-matrix check in `scripts/check_features.sh`.
4. Update this table and any example fixtures that intentionally consume the new
   backend.

