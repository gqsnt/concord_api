# Concord v2.1 release verification

> **Historical candidate evidence — not current exact-HEAD evidence.** This
> record is scoped to the SHA below and does not establish the current beta or
> final release state.

- **Recorded:** 2026-09-04
- **Implementation candidate:** `9470e24034c9d421d937c631adee1a4a67c4e212`
- **Branch:** `release/v2.1-readiness`
- **Version:** `2.1.0-rc.1`
- **Clean worktree:** `F:\projects\concord_api2_candidate_measure`, detached at the implementation candidate
- **Host/toolchain:** Windows x86-64; Cargo 1.97.1 (`c980f4866`); rustc 1.97.1 (`8bab26f4f`); MSRV 1.86.0

## Release result

Not stable-ready and not merged. Gates 3 and 10 still require independent
human evidence. In addition, `cargo deny` correctly fails because the four
public Concord packages have no repository license grant. The repository and
observed history contain no SPDX project license or license file, so no license
or allowlist waiver was invented.

## Baseline and preservation

The starting branch was `main` at `f7464c5`, ahead of `origin/main` by three
commits, with a large modified/untracked v2.1 tree. Initial `git diff --check`
passed. Cargo/rustc were 1.97.1. The available baseline `just fmt`, `just
check`, and `just test` commands passed before semantic edits. The exact
intended dirty prototype was committed on preserved branch
`backup/v2.1-prototype-unreviewed` as `50460e7`; work continued on
`release/v2.1-readiness` at RC version `2.1.0-rc.1`.

## Verification commands and outcomes

| Command | Location | Exit/result |
|---|---|---|
| `just fmt` | development/aggregate | 0 |
| `just check` | development/aggregate | 0; default, no-default, all-feature, isolated-feature, metadata, and policy shapes passed |
| `just lint` | development/aggregate | 0 with warnings denied |
| `just test` | clean aggregate | 0; 879 default and 904 all-feature tests passed |
| `just ui` | clean aggregate | 0; 189 macro tests and no-default/all-feature/core trybuild suites passed |
| `just msrv` | clean aggregate | 0; all Rust 1.86 library shapes and semantic UI passed |
| `just fixtures` | clean aggregate | 0; dependency/feature graph fixtures passed |
| `just docs` | clean aggregate | 0; 29 source anchors, 56 labeled pseudocode fences, doctests, and every warning-denied Rustdoc feature shape passed |
| `just deny` / `cargo deny check` | development and clean aggregate | 1; advisories/bans/sources OK, licenses FAILED for `concord`, `concord-core`, `concord-extras`, and `concord-macros` |
| `just workflow` | development and clean candidate | 0; `actionlint` passed |
| `just analyzer` | development and clean candidate | 0; analysis/IDE exercise and diagnostics completed, with only inactive-code weak warnings |
| `just bench` | development and clean candidate | 0; benchmark target compiled |
| `just verify` | development and clean candidate | 1 at `deny` after every preceding recipe passed; later recipes passed individually |
| `cargo test --workspace --all-features --locked` | development at `9470e24` | 0 |
| `cargo rustdoc -p concord --all-features -- -D warnings` | clean aggregate | 0 |
| `cargo rustdoc -p concord-extras --all-features -- -D warnings` | clean aggregate | 0 |
| `cargo run -p concord-xtask -- migration-audit` | development | 0; 19 intentional opaque-payload review markers, no rewrite |
| `cargo run -p concord-perf --release --bin expansion_scale --locked` | development at `9470e24` | 0; 1/10/100/500/1,000 endpoints completed |
| `cargo run -p concord-perf --release --bin stream_driver_comparison --locked` | development at `9470e24` | 0; all precommitted budgets passed |
| `cargo bench -p concord-perf --bench concord_overhead` | development at `9470e24` | 0; 17/17 named categories, 100 iterations each |
| `pwsh -NoLogo -NoProfile -File scripts/verify_packaged_downstream.ps1` | development and clean candidate | 0; four archives packaged/extracted, downstream check and run used extracted sources only |
| `git diff --check` | development candidate | 0 |

Earlier aggregate attempts are retained as failure evidence. Repository policy
first rejected the newly curated evidence directory (`b1b6b7d` corrected the
raw/curated distinction); Clippy found an unnecessary return in the injected
test clock (`e862270`); and the all-feature compiler listed one extra trait in
a retired-`accept` diagnostic (`af31e0c`). Each defect received a focused fix
and rerun. The later license failure was not suppressed.

`cargo publish --dry-run` was not run. Exact RC inter-crate versions do not
exist in the registry, so the maintained archive/extract/downstream check gives
the relevant local evidence without a remote action. No push, publish, tag, or
release operation occurred.

## Remaining external actions

1. At least three unfamiliar Rust developers must complete
   `dev_doc/dx_study/`, and results must meet its precommitted rubric.
2. A relevant non-primary reviewer must complete `dev_doc/rustdoc_review/`.
3. The project owner must select and supply the project license, after which
   the public package manifests and `cargo deny` can be made consistent.
4. Rerun the exact clean suite. Only if all three evidence/authority gaps close
   may the version become `2.1.0` and the release branch be merged to `main`.
