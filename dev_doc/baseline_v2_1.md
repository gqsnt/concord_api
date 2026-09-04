# Concord v2.1 implementation baseline

> **Historical baseline evidence — not current source of truth.** This records
> the pre-v2.1 repository state only.

Recorded 2026-09-03 before v2.1 semantic edits.

## Observed repository state

- The supplied worktree was clean at commit `f7464c5`.
- Contrary to the implementation kit's expected v1/1.0.0 input, the checked-in
  workspace identified itself as 2.0.0 and contained a coherent earlier v2
  candidate. That candidate is implementation evidence only; it is not retained
  as a public intermediate language.
- The workspace contained 680 tracked files and 107,205 lines reported by
  rust-analyzer for workspace crates.
- The generated contract bridge version was 32.
- The active language used `using`, `repeatability`, `timeout off`, and
  `clear_x`, and its representation markers included payload types such as
  `Json<T>`.
- The scale probe covered 1, 10, 100, and 1,000 endpoints; the mandatory
  500-endpoint point was absent.

## Baseline verification

`just verify` completed successfully with exit code 0. It ran the maintained
format, check, lint, test, UI, MSRV, fixtures, docs, deny, workflow, analyzer,
and benchmark-build gates.

Observed highlights:

- 874 main nextest tests passed.
- 187 `concord-macros` tests passed.
- façade UI suites passed without and with all features; the core UI suite
  passed.
- Rust 1.86.0 MSRV checks and semantic MSRV UI checks passed.
- fixture graph, documentation anchors/links, doctests, and `-D warnings`
  rustdoc variants passed.
- dependency advisories, bans, licenses, and sources passed. `cargo deny`
  reported warning-only duplicate versions of `base64`, `core-foundation`,
  `syn`, and `windows-sys`.
- `actionlint`, both rust-analyzer commands, and the benchmark compile probe
  passed.

The full terminal stream was intentionally not committed because cargo
metadata and analyzer progress are very large and machine-specific. This file
records the stable evidence needed to compare the migrated workspace.

## Prototype budgets

These are evidence budgets, not semantic contracts:

- Representation traits must compile on Rust 1.86.0 without coherence overlap
  or public native-response authority.
- Ordinary endpoint output must contain no decoder/driver wrapper.
- The scale probe must cover 1/10/100/500/1,000 endpoints and remain bounded by
  its existing 1,000-endpoint maximum.
- Stream acquisition must retain lazy polling, bounded empty-chunk behavior,
  backpressure, and synchronous cancellation on drop.
- Generated metadata remains static/shared unless Gate 10 demonstrates an
  unacceptable compile or binary-size cost.
