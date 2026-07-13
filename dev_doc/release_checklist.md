# Release Checklist

- Run `just release` and `git diff --check`.
- Confirm Rust 1.97 and Reqwest `=0.13.4` remain aligned in root and `perf`.
- Inspect generated-source assertions and Trybuild diagnostics.
- Confirm `concord_core::__private` is the only generated integration path and
  contains no numeric module or migration bridge.
- Confirm `advanced` exports only documented extension points.
- Confirm loopback execution support remains development-only.
- Review documentation, examples, feature topology, and performance targets
  for removed migration language.
- Review lockfile changes and retain them only for demonstrated dependency
  changes.
