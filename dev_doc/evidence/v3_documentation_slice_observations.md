# v3 documentation-slice observations — 2026-09-26

**Status:** Observed command results recorded while writing the v3 maintainer
records. They cover only the commands below, on the source identity below. They
are not the complete v3 verification matrix, which is recorded in the
implementation report (`IMPLEMENTATION_REPORT.md`, produced separately), and
they establish no gate listed in [`../release_gate_status.md`](../release_gate_status.md).

## Environment and source identity

- Host: Linux x86_64 (kernel 6.18); `rustc 1.95.0 (59807616e 2026-04-14)`,
  `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`, stable channel, offline vendored
  dependencies, `--locked`.
- Environment: `RUSTUP_TOOLCHAIN=stable`, `CARGO_INCREMENTAL=0`,
  `CARGO_BUILD_JOBS=2`, a private `CARGO_TARGET_DIR`.
- Tested commits: `6f90a9c383d93aa6db87baa6734bee94f11c1af3` (runs 1–5) and
  `66771d506e1665afe26de9c0ad5b542bb2fe821f` (runs 6–8), both on the
  documentation branch. Neither commit changes anything outside `dev_doc/` and
  `FINAL.md` relative to the integrated `v3-implementation` commit
  `275a345` (`git diff --stat 275a345 HEAD -- crates tools tests fixtures scripts
  Cargo.toml Cargo.lock justfile README.md docs` printed nothing), so the code
  under test is that commit's code.

## Results

| # | Command | Exit | Result |
|---|---|---:|---|
| 1 | `cargo test --locked -p concord-macros --lib` | 0 | 31 passed, 0 failed |
| 2 | `cargo test --locked -p concord-xtask` | 0 | unit 40 passed; `tests/contract_cli.rs` 2 passed; `tests/migrated_v3.rs` 8 passed; 0 failed |
| 3 | `cargo test --locked -p concord-core --lib --features multipart` | 0 | 665 passed, 0 failed |
| 4 | `cargo test --locked -p concord-core --lib --all-features` | 0 | 818 passed, 0 failed |
| 5 | `cargo test --locked -p concord-extras --all-features` | 0 | unit 96 passed; `tests/contracts.rs` 22 passed; doctests 2 + 1 (compile-fail) passed; 0 failed |
| 6 | `cargo test --locked -p concord-downstream-app` | 0 | `tests/downstream.rs` 4 passed; doctests 1 + 4 (compile-fail) passed; 0 failed |
| 7 | `cargo run --locked -p concord-xtask -- docs-check` | 1 | `xtask: could not read …/crates/concord-macros/src/semantic/symbols.rs: No such file or directory` |
| 8 | `cargo run --locked -p concord-xtask -- release-audit` | 1 | Same message: `release-audit` runs `docs-check` first and stops there |

Run 2 includes the `release_audit` unit test
`v3_beta_lifecycle_requires_the_exact_version_and_named_blockers` and the
migration tests cited by ADRs 0014, 0015 and 0022. Runs 3–4 include the core
tests cited by ADRs 0014–0026 whose files live under `crates/concord-core/src/`
(for example `crates/concord-core/src/auth/slot/intent_tests.rs`,
`crates/concord-core/src/request/program.rs`,
`crates/concord-core/src/request/runtime_endpoint/tests.rs` and
`crates/concord-core/src/pagination/runtime_tests/`). Run 6 is the two-crate
downstream package cited by ADR 0026. Tests in `crates/concord/tests/`,
`crates/concord-examples/tests/`, `crates/concord-inspect/` and the trybuild UI
suites were **not** run by this slice.

"Cited by" refers to the ADR citations as they stood when these runs were
recorded. The tests that ADRs 0011, 0014 and 0021, the decision record and
claims C68–C70 cite for the external review's findings F01–F03 were added
later (after `fc3e4d1`) and were not run by this slice; their results are
recorded in the r2 validation run.

## Why `docs-check` and `release-audit` fail (runs 7–8)

`docs-check` reads `crates/concord-macros/src/semantic/symbols.rs` (and
`emit/descriptions.rs`), which the v3 macro no longer has; the checker is being
converted separately. To see whether any remaining finding concerns the
maintainer records, a scratch copy of the tree (outside the repository, not
committed) was run with the checker's file-read helper made non-fatal. That
diagnostic run reported no finding in `dev_doc/` or `FINAL.md`; its findings
were all in `README.md`, `docs/`, example/fixture sources, core sources
(the retired-symbol substring scan for `response_bytes` also matches v3
identifiers such as `cap_response_bytes` and `acquired_response_bytes` in
`controls.rs`, `pagination/engine.rs`, `pagination/summary.rs`,
`runtime/config.rs` and tests) and the v2.1 agreement rules (optional-secret
message, rate-diagnostic text, `clear_*` scan matching the v3
`credentials().clear_<credential>()` API). The diagnostic run is not a
`docs-check` result.

Even with `docs-check` passing, `release-audit` would stop at
`git merge-base main HEAD` (`fatal: Not a valid object name main`, exit 128):
the repository has no `main` branch. The audit's remaining non-Git checks were
replicated with `grep` on commit `66771d5` and hold: no obsolete root process
artifacts; no `todo!`/`unimplemented!`/placeholder markers in `crates/` or
`tools/`; `SystemTime::now()` appears in `crates/concord-core/src/` only in
`runtime/wall_clock.rs`; observer events carry `pub target: RequestTarget` and
the limiter carries `pub logical_origin: Option<&'a str>` without URL/host/header
fields; the four product manifests set `publish = false`; and
`release_gate_status.md` names `` `3.0.0-beta.1` ``, `integration candidate /
beta` and `Blocked — owner authority required`. A grep replication is not the
command's result.
