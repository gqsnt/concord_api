# Phase 13 evidence: documentation, workspace, tooling, and CI

Phase 13 started from the Phase 12 commit `62278d1`. The late package-path
move was isolated in commit `1ffb54c`; this document records the complete
Phase 13 worktree before its documentation/tooling completion commit.

## Outcome

The repository now exposes its architecture at the root:

```text
crates/
  concord
  concord-core
  concord-extras
  concord-macros
  concord-test-support
  concord-examples
fixtures/
tools/
  xtask
  perf
docs/
  guide
  concepts
  recipes
  reference
  contributor
```

The main workspace has no nested product workspace. `fixtures` is the sole
separate Cargo workspace. Only the root and fixture lockfiles exist.

The user manual now begins with representation, generated Rust, then client
construction/execution. It contains seven guides, five concepts, four recipes,
eight reference pages, and five primary contributor pages. Every guide links
to an importable example and proving test. Exact façade features and runtime
numeric defaults are generated/checked against their manifest/core source.
Historical phase evidence moved under `docs/contributor/evidence` and remains
non-normative.

`tools/xtask` provides portable `docs` and `fixtures` tasks with no ecosystem
dependencies. The documentation task checks generated tables, relative links,
guide proof links, the `justfile` inventory, and retired/implementation
vocabulary in user pages. The `justfile` no longer selects PowerShell or
requires Nextest; direct Cargo commands are the primary path.

`.github/workflows/ci.yml` defines explicit format, stable, all-feature,
no-default, MSRV, Clippy, generated/UI, lifecycle/security, extras, fixtures,
examples, docs, three-platform, cargo-deny, and non-blocking performance lanes.
CI itself was not executed from this local worktree.

All packages remain `publish = false` intentionally. No license, canonical
repository URL, publication owner, or ownership of the occupied public
`concord` name is approved, so Phase 13 does not fabricate release metadata.

## Validation

Executed on Windows MSVC with pinned Rust 1.97.1 unless another toolchain is
shown.

| Command | Result |
|---|---|
| `cargo metadata --format-version 1 --all-features` plus fixture metadata after the path-only commit | PASS |
| `cargo check --workspace --all-targets --locked` | PASS |
| `cargo check --workspace --all-targets --all-features --locked` | PASS |
| `cargo test --workspace --locked` | PASS |
| `cargo test --workspace --all-features --locked` | PASS; combined clean rerun completed in 207.3 s |
| `cargo run --locked -p concord-xtask -- docs` | PASS |
| `cargo run --locked -p concord-xtask -- fixtures` | PASS; fixture metadata and all six public compile groups pass |
| `cargo test --workspace --doc --locked` | PASS; three compile-fail doctests |
| `cargo test --workspace --doc --all-features --locked` | PASS; five compile-fail doctests |
| `cargo rustdoc -p concord --all-features --locked -- -D warnings` | PASS |
| `cargo rustdoc -p concord-core --all-features --locked -- -D warnings` | PASS |
| `cargo rustdoc -p concord-extras --all-features --locked -- -D warnings` | PASS |
| `cargo +1.85.0 check --workspace --all-targets --no-default-features --locked` | PASS |
| `cargo +1.85.0 test -p concord-core --lib --no-default-features --locked` | PASS; 254 tests |
| `cargo +1.85.0 test -p concord-extras --all-features --locked` | PASS; 37 unit, 7 integration, and one doctest |
| `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS on the same Rust 1.97.1 commit after narrow lint corrections |
| `cargo test -p concord-macros --locked` plus façade generated-surface, dependency-policy, public-extension, and all-feature UI targets | PASS after the lint corrections; 61 macro and 22 façade owner tests |
| `just --list` | PASS; all documented portable recipes present |
| `cargo deny check` | PASS with reviewed duplicate `core-foundation` and `windows-sys` warnings |
| `cargo bench -p concord-perf --bench concord_overhead --no-run --locked` | PASS |
| `cargo bench -p concord-perf --bench concord_overhead --locked -- --smoke` | PASS; all 12 named categories executed |
| `cargo fmt --all -- --check` | PASS in final gate |
| `git diff --check` | PASS in final gate |

The smoke timings are observational single iterations:

| Category | Time |
|---|---:|
| core client construction | 228.1 µs |
| generated client/scope/request construction | 8.3 µs |
| safe public planning | 32.9 µs |
| public-head preparation | 34.6 µs |
| codec encode/decode | 0.8 µs |
| body-limit polling | 2.8864 ms |
| ready credential preparation/finalization | 39.1 µs |
| ready limiter acquisition | 8.5114 ms |
| pagination loop | 2.6555 ms |
| stream wrapping | 0.8 µs |
| generic specification selection | 15.4 µs |
| record framing | 0.6 µs |

These timings are not thresholds and are not cross-machine comparisons.

## Unavailable or interrupted checks

- The unqualified pinned-toolchain `cargo clippy` invocation is unavailable:
  the local exact-channel installation reports that `cargo-clippy.exe` is not
  applicable. The installed `stable` alias reports the same Rust 1.97.1 commit
  and its Clippy component ran the complete warning-denied command
  successfully. The CI lane still installs its component explicitly.
- `actionlint` and a local YAML parser were unavailable. Python was not
  installed, PowerShell had no `ConvertFrom-Yaml`, and Node had no YAML module.
  The workflow file was reviewed but is not claimed parser-validated or
  executed.
- Linux/macOS platform lanes and the GitHub Actions matrix cannot be executed
  from this Windows worktree.
- The interactive rust-analyzer completion/navigation protocol remains a
  manual release gate; command-line compilation and Rustdoc are not presented
  as substitutes.
- Two early combined workspace-test invocations used command timeouts that
  killed Cargo and produced a Windows broken-pipe message. The same exact test
  commands were rerun cleanly with a ten-minute command budget and passed.
- Product `cargo package` dry runs remain intentionally deferred: every
  package is unpublished and exact path siblings plus absent approved release
  metadata make a registry package proof non-representative.

The localized MSVC proc-macro linker message remains a warning in ordinary
build/test output. No reference deviation was required.
