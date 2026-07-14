# Release Checklist

- Run `just release` and `git diff --check`.
- Confirm Rust 1.97 and Reqwest `=0.13.4` remain aligned in root and `perf`.
- Inspect generated-source assertions and Trybuild diagnostics.
- Confirm `concord_core::__private` is the only generated integration path and
  contains only opaque descriptors/adapters and narrow prepare/execute entry
  points; generated output must contain no runtime-plan construction.
- Confirm `advanced` exports only documented extension points.
- Confirm generic clients cannot install status retry and generated status
  eligibility is derived from the emitted API descriptor without a global
  capability token.
- Confirm loopback execution support remains development-only.
- Review documentation, examples, feature topology, and performance targets
  for current terminology and contracts.
- Review lockfile changes and retain them only for demonstrated dependency
  changes.
