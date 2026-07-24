# Closure C7 executable validation and documentation truth

Phase:
Closure C7 — Add executable CI, passing MSRV proof, and truthful
validation/documentation.

Started from commit:
`f5fec6e3886143f9f304ba9c6514cabe02f03da2`
(`build: remove test support from Concord product graph`) on branch `master`.

Finished at commit:
The C7 commit containing this evidence, with message
`ci: make Concord closure validation executable`. Its exact hash will be
backfilled in the closure ledger by C8.

Files changed:

- `.github/workflows/ci.yml`;
- `CHANGELOG.md`;
- `Cargo.lock`;
- `README.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `crates/concord-core/src/auth/placement.rs`;
- `crates/concord-core/tests/ui/msrv/expectations.txt`;
- `crates/concord-core/tests/ui/msrv/forbidden.txt`;
- `crates/concord-core/tests/ui_msrv_semantic.rs`;
- `crates/concord-macros/src/emit/endpoints.rs`;
- `crates/concord-test-support/Cargo.toml`;
- `crates/concord-test-support/src/cargo_fixture.rs`;
- `crates/concord/tests/ui/msrv/expectations.txt`;
- `crates/concord/tests/ui/msrv/forbidden.txt`;
- `crates/concord/tests/ui_msrv_semantic.rs`;
- `docs/contributor/documentation.md`;
- `docs/contributor/evidence/closure-c7-validation-ci.md`;
- `docs/contributor/evidence/refactor-phase-0-baseline.md`;
- `docs/contributor/evidence/refactor-phase-13-docs-tooling.md`;
- `docs/contributor/evidence/refactor-phase-14-release.md`;
- `docs/contributor/evidence/refactor-phase-5-namespaces-features.md`;
- `docs/contributor/evidence/rust-analyzer-acceptance.md`;
- `docs/contributor/release.md`;
- `docs/contributor/testing.md`;
- `fixtures/Cargo.lock`;
- `fixtures/Cargo.toml`;
- the ten semantic-UI wrapper binaries and two manifests under
  `fixtures/msrv`;
- `justfile`;
- `tools/xtask/src/main.rs`.

Design decisions:

- Stable Rust remains the sole owner of exact Trybuild snapshots. Rust 1.85
  owns separate compiler-JSON assertions for ten representative semantic
  failures: five façade cases and five core cases.
- Each semantic case compiles as a separate fixture binary. The harness
  requires failure, all expected user-facing fragments in one compiler error,
  an expected primary source-file suffix, and absence of every configured
  bridge/sentinel token from all compiler errors.
- Expectations and forbidden tokens are separate text owners. Failure output
  is sanitized and capped at 65,536 bytes before a test panic can retain it.
- The semantic fixture packages live in the already isolated `fixtures`
  workspace. The root workspace still excludes that workspace.
- The workflow exposes all fifteen required job IDs. Every third-party action
  uses a lowercase 40-hex revision, every job has a timeout, correctness
  commands are locked, caches include OS/toolchain/lockfile facts, and failed
  stable-UI, MSRV, and package jobs retain logs.
- The final CI job reruns documentation generation/checking and rejects
  whitespace errors, tracked differences, and untracked files. It is not
  placed in an ordinary dirty developer gate.
- Just now separates stable-local, MSRV, package, and release-local proof.
  The xtask parser inspects dependency edges and command bodies, including
  toolchain-qualified Cargo commands and YAML `run:` commands, rather than
  accepting recipe/job names alone.
- Documentation checks include evidence files, local Markdown targets, and
  Markdown anchors. External URLs are counted but deliberately not fetched.
- Two compiler-equivalent source rewrites remove rust-analyzer false
  diagnostics: authentication-header collision matching compares the
  borrowed header explicitly, and generated pagination implementations use
  the concrete controller type in trait method signatures.
- The façade-owned generated macro bridge remains unchanged. An attempted
  analyzer-only path adjustment was reverted because it did not solve the
  nested expansion limitation and would weaken the frozen façade boundary.

Audit findings closed:

- F-03 is closed in source: executable CI exists, immutable action revisions
  are policy-checked, Actionlint passes locally, and all required logical
  owners are explicit. Remote execution remains separately pending.
- F-06 is closed: stable exact UI and both Rust 1.85 semantic diagnostic
  owners pass.
- F-13 is closed: the overloaded `just verify` owner is removed; explicit
  locked scopes and parsed command ownership replace it.
- F-14 is closed for maintained source: broken evidence links and Phase
  13/14 overclaims are corrected, evidence participates in link checking,
  anchors are checked, and external URL scope is stated exactly.
- F-15 is closed: the README states that packages are unpublished before
  showing a path checkout dependency and labels registry syntax as future.
- The validation/documentation portion of F-04 is locally implemented.
- F-07's automated command-line portion is executed. Interactive editor
  acceptance is not closed and remains an external release check.

Tests added/changed:

- Added façade and core `ui_msrv_semantic` integration owners.
- Added ten isolated fixture binaries that reuse the corresponding stable UI
  source files without copying their diagnostic cases.
- Added semantic-JSON parser, primary-span, forbidden-token sanitization, and
  bounded-failure-output coverage to `concord-test-support`.
- Added an xtask unit owner proving locked-command policy recognizes
  toolchain-qualified Cargo commands and YAML `run:` steps while exempting
  formatting and cargo-deny.
- Existing macro exact UI, façade exact UI with and without default features,
  and core exact UI passed without snapshot changes.
- Repository policy now checks workspace isolation, real Just ownership,
  required workflow jobs, timeouts, immutable action revisions, cache facts,
  locked commands, semantic owners, package ownership, and clean-tree facts.

Commands executed:

```text
cargo test -p concord-test-support --lib --no-default-features --features cargo-fixture --locked
cargo test -p concord-macros --lib --locked
cargo +stable test -p concord --test ui --no-default-features --locked
cargo +stable test -p concord --test ui --all-features --locked
cargo +stable test -p concord-core --test ui --no-default-features --locked
cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked
rust-analyzer --version
rust-analyzer -q analysis-stats . --run-all-ide-things
rust-analyzer -q diagnostics .
just --list

cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --all-features --locked
cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo +1.85.0 test -p concord-core --lib --no-default-features --locked
cargo +1.85.0 test -p concord-extras --all-features --locked
cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked
cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- docs
cargo run --locked -p concord-xtask -- docs --write
cargo run --locked -p concord-xtask -- package
cargo test -p concord-xtask --locked
cargo +stable clippy -p concord-xtask --all-targets --locked -- -D warnings
cargo +1.85.0 check -p concord-xtask --all-targets --locked
cargo test --workspace --doc --locked
cargo test --workspace --doc --all-features --locked
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings
cargo deny check
cargo run --locked -p concord-xtask -- policy

Get-FileHash -Algorithm SHA256 -LiteralPath target/c7-tools/actionlint_1.7.12_windows_amd64.zip
actionlint --version
actionlint .github/workflows/ci.yml
git diff --check
git status --short
```

The first `cargo test --workspace --locked` attempt was interrupted by the
user and has no inferred result. Process inspection found no remaining Cargo,
rustc, or test process. The command was restarted from the beginning and the
clean rerun is the result recorded below. Read-only review also used `rg`,
`Get-Content`, `git diff`, `git status`, and direct inspection of every new
expectation, wrapper, manifest, action pin, and documentation correction.

Exact results:

- Formatting, locked metadata, and the default/no-default/all-feature
  all-target checks exited 0.
- Warning-denied stable Clippy exited 0. Rust 1.97's localized MSVC
  `linker_messages` warning remained visible and is explicitly not promoted
  by `-D warnings`.
- The restarted default workspace suite and the all-feature workspace suite
  exited 0. All unit, integration, exact UI, semantic UI, rendered Rustdoc,
  and doctest owners selected by those commands passed.
- Direct stable exact owners exited 0: macros had 67 unit tests; façade UI had
  6 no-default and 8 all-feature harness tests; core had all 3 exact UI
  harnesses. No `.stderr` snapshot changed.
- Rust 1.85 all-target/no-default workspace compilation exited 0. Core
  no-default library tests passed 285 tests. Extras all-feature tests passed
  46 unit tests, six integration owners, and one compile-fail doctest.
- Both Rust 1.85 semantic integration owners exited 0. Each compiled and
  validated five separate expected-failure binaries.
- Test-support's cargo-fixture unit owner exited 0 with 3 passed.
- Xtask's locked-command policy unit owner exited 0 with 1 passed.
  Its targeted warning-denied stable Clippy and Rust 1.85 all-target checks
  also exited 0 after the final parser change.
- The fixture driver exited 0: locked fixture metadata passed and all 7
  serialized no-default public-compile cases passed.
- Documentation ownership exited 0: 140 local file links and 2 local anchors
  were checked; 0 external URLs were syntax-counted and none were fetched.
  A final generation/check cycle left the complete intentional tracked and
  untracked patch unchanged.
- Local package verification exited 0 in the order core, macros, extras,
  façade. All four archives and normalized manifests were inspected, and all
  four extracted libraries compiled with all features and locked sibling
  resolution. Cargo warned that legal and repository metadata is absent.
- Default doctests exited 0 with the two core and one extras compile-fail
  owners passing. All-feature doctests exited 0 with four core and one extras
  compile-fail owners passing.
- Warning-denied Rustdoc for façade, core, and extras each exited 0.
- `cargo deny check` exited 0: advisories, bans, licenses, and sources were
  accepted. The two reviewed duplicate-version warnings were
  `core-foundation` 0.9.4/0.10.1 and `windows-sys` 0.52.0/0.61.2.
- Actionlint 1.7.12 exited 0 for the workflow. Its official Windows-amd64
  archive SHA-256 was
  `6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9`.
  The archive and extracted binary remain under ignored `target/c7-tools`.
- Repository policy and the final documentation-policy rerun exited 0 after
  the Cargo-command parser was tightened.
- Rust-analyzer 1.97.1 `analysis-stats --run-all-ide-things` exited 0 over 50
  crates, 339 workspace item trees, and 74,999 workspace lines. The root
  analysis contained zero `fixtures/msrv` paths.
- `rust-analyzer -q diagnostics .` exited 1. It reported 118 unresolved macro
  calls, all at the façade feature-selected generated raw-response macro, and
  cascaded to E0282=209, E0432=52, E0277=28, and E0599=5. The two prior
  authentication-header move diagnostics and pagination controller field
  diagnostics are absent. Compiler checks and exact UI owners pass; this is
  recorded as a command-line analyzer expansion limitation, not a passing
  diagnostic result.
- The observed pre-evidence `git diff --check` exited 0. Final diff and status
  truth checks are rerun immediately before the phase commit.

Unexecuted checks:

- No graphical editor session was available. All nine manual rust-analyzer
  rows are marked `Pending external editor session` in the companion
  evidence.
- No GitHub Actions run and no independent remote Linux, Windows, or macOS
  lane was executed because nothing was pushed.
- External Markdown URLs were not fetched. The maintained documentation
  currently contains zero syntax-counted external Markdown URL targets.
- No registry ownership, public package-name approval, license choice,
  copyright decision, or repository/homepage metadata decision was made.
- No package was published and no branch was pushed.

Security/redaction review:

- Semantic diagnostic errors are checked for all configured internal bridge
  tokens and the protected sentinel. Failure rendering replaces every
  forbidden token and caps retained output before panicking.
- Expectations use structural compiler messages and source-path suffixes;
  they contain no credential, authorization value, rate key, protected URL,
  or native transport value.
- CI failure artifacts contain compiler/package command output only. The test
  matrix continues to use deterministic local transports and no deterministic
  test calls an external service.
- The two source rewrites are compiler-equivalent and do not alter endpoint
  construction, authentication material, planning, transport, recovery,
  completion, cancellation, response acquisition, or pagination behavior.
- Full default/all-feature workspace, lifecycle/security/bounds, exact UI,
  semantic UI, fixture, and package owners passed.

Performance impact:

- C7 changes validation/tooling and compiler-facing equivalent source forms;
  it adds no runtime allocation, transport, authentication, limiting, or
  executor work.
- Semantic UI adds ten isolated compiler invocations and bounded diagnostic
  parsing to validation time. Its target directories are isolated and finite.
- No numeric runtime performance comparison is claimed in C7. C8 owns the
  final observational comparison.

Deviations:
None. The non-green command-line rust-analyzer diagnostic result is retained
explicitly; it was not bypassed by changing the frozen façade macro boundary
or recategorized as a passing check.

Remaining blockers:

- Owner: repository maintainer with a graphical Rust editor. Run and record
  all nine manual acceptance rows in
  `docs/contributor/evidence/rust-analyzer-acceptance.md`.
- Owner: repository maintainer with push and Actions authority. Push an
  approved branch or open a pull request, run the pinned workflow, and record
  every required job plus the Linux/Windows/macOS matrix result.
- Owner: repository maintainer and, if reproduced independently, upstream
  rust-analyzer. Recheck the façade nested macro expansion in an editor/newer
  analyzer and file an upstream issue if the 118 unresolved expansions
  remain.
- Owner: release/legal owner. Choose legal and repository metadata, approve
  registry names/ownership, and explicitly decide whether to enable
  publication.
