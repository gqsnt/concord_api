# Closure C0 truth-reset baseline

Phase:
Closure C0 — Reset source truth and establish the remaining baseline.

Started from commit:
`22667bc9314e3e47f324d9c5685bf6219f00ef46`
(`refactor: harden final concord contract`) on branch `master`.

Finished at commit:
`f5ce4bb2ed58215055b6a986fa76d97c4951728d`
(`docs: establish Concord refactor closure baseline`).

Files changed:

- `CONCORD_REFACTOR_AUDIT.md` (retained at the repository root);
- `CONCORD_REFACTOR_CLOSURE_AGENT_PROMPT.md` (retained at the repository root);
- `CONCORD_REFACTOR_CLOSURE_PLAN.md` (retained and ledger updated);
- `CONCORD_REFACTOR_REFERENCE.md` (remaining-work notice and corrected original ledger);
- `docs/contributor/evidence/closure-c0-baseline.md`.

Design decisions:

- The accepted façade/core/extras/macros architecture and runtime lifecycle
  remain frozen.
- The audit's F-03 statement is archive-specific. The current Git checkout
  already tracks `.github/workflows/ci.yml`; C7 must validate and strengthen
  that workflow because it does not yet own the closure plan's
  actionlint/policy/package/release gates.
- Historical evidence files were not rewritten. The original progress ledger
  now distinguishes substantially implemented architecture from unresolved
  closure criteria.
- C0 changes documentation and evidence only; no product source, manifest,
  test, fixture, workflow, or generated snapshot changed.

Audit findings closed:

- No product finding is closed by C0. F-04's overstated completion claim is
  reset to an evidence-backed blocked state; final release proof remains
  assigned to C8.
- The original ledger now reports Phase 9 and Phase 13 as incomplete and
  Phase 14 as blocked.
- Original Phases 3, 4, 10, and 12 are marked substantially implemented with
  their specific closure issues still open.
- All F-01 through F-18 findings are assigned to C1–C8 in the closure plan.

Tests added/changed:
None. C0 is a documentation and baseline phase.

Commands executed:

```text
git rev-parse --show-toplevel
git branch --show-current
git rev-parse HEAD
git status --short
git log --oneline --decorate -20
git ls-files --error-unmatch .github/workflows/ci.yml
rustc --version --verbose
cargo --version
rustup --version
just --version
cargo deny --version
rust-analyzer --version
cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord --all-features --locked
cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- docs
cargo package --locked -p concord-test-support
cargo package --locked -p concord-macros
cargo package --locked -p concord-core
cargo package --locked -p concord-extras
cargo package --locked -p concord
cargo +1.85.0 test -p concord --test ui --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui --no-default-features --locked
rust-analyzer diagnostics .
git diff --check
git diff --exit-code
```

Two ancillary source-discovery commands were also executed. The first used
PowerShell-incompatible wildcard path arguments and exited 1 after returning
the workflow matches; it was rerun successfully with Cargo-manifest globs
owned by ripgrep:

```text
rg -n "^(name:|on:|  workflow_dispatch:)|runs-on:|uses:|actionlint|package|release|policy" .github/workflows/ci.yml; rg -n "dangerous-dev-tools|concord-test-support" crates/*/Cargo.toml tools/*/Cargo.toml Cargo.toml
rg -n -g Cargo.toml "dangerous-dev-tools|concord-test-support" .
```

Exact results:

- Git root: `F:/projects/concord_api`; branch: `master`; starting HEAD:
  `22667bc9314e3e47f324d9c5685bf6219f00ef46`.
- Starting status contained exactly three untracked authoritative closure
  inputs: `CONCORD_REFACTOR_AUDIT.md`,
  `CONCORD_REFACTOR_CLOSURE_AGENT_PROMPT.md`, and
  `CONCORD_REFACTOR_CLOSURE_PLAN.md`. There were no tracked diffs.
- The expected final refactor commit exists at starting HEAD:
  `22667bc refactor: harden final concord contract`; the prior Phase 14
  implementation commit is `c21ed3c`.
- `.github/workflows/ci.yml` exists and is tracked at starting HEAD. It has
  Linux jobs and an OS matrix, but uses mutable action tags and has no
  actionlint/policy/package/release-clean owner required by C7.
- Toolchain: `rustc 1.97.1 (8bab26f4f 2026-07-14)`,
  `cargo 1.97.1 (c980f4866 2026-06-30)`,
  `rustup 1.29.0`, host `x86_64-pc-windows-msvc`, LLVM 22.1.6.
- Environment: PowerShell 7.6.4 on
  `Microsoft Windows 10.0.26200 Win32NT` (Windows 11 build); Just 1.46.0;
  cargo-deny 0.20.2; rust-analyzer 1.97.1.
- `cargo fmt --all -- --check`: exit 0.
- `cargo metadata --format-version 1 --all-features --locked`: exit 0.
- `cargo check --workspace --all-targets --all-features --locked`: exit 0;
  the localized MSVC linker warning remained visible.
- `cargo test -p concord-core --lib --all-features --locked`: exit 0,
  329 passed.
- `cargo test -p concord-extras --all-features --locked`: exit 0; 37 unit
  tests plus all integration tests and doctests passed.
- `cargo test -p concord --all-features --locked`: exit 0; all runtime,
  integration, rustdoc, public-compile, and stable exact UI owners passed.
- `cargo +1.85.0 check --workspace --all-targets --no-default-features --locked`:
  exit 0.
- `cargo run --locked -p concord-xtask -- fixtures`: exit 0.
- `cargo run --locked -p concord-xtask -- docs`: exit 0.
- `cargo package --locked -p concord-test-support`: exit 0.
- `cargo package --locked -p concord-macros`: exit 0.
- `cargo package --locked -p concord-core`: exit 101 because registry
  preparation could not resolve unpublished `concord-test-support = 0.1.0`.
- `cargo package --locked -p concord-extras`: exit 101 because registry
  preparation could not resolve unpublished `concord-core = 0.1.0`.
- `cargo package --locked -p concord`: exit 101 because registry preparation
  could not resolve unpublished `concord-core = 0.1.0`.
- Rust 1.85 façade exact UI: exit 101 with 19 rendering mismatches
  (5/6 pagination, 10/13 capability/privacy, 4/9 authentication/rate-limit).
  Stable snapshots were not changed.
- Rust 1.85 core exact UI: exit 101 with 14 rendering mismatches
  (3 dangerous-feature, 10/14 source-only, 1 multipart). Stable snapshots
  were not changed.
- `rust-analyzer diagnostics .`: exit 1. It reported source diagnostics,
  including generated pagination-controller field errors (`E0559`, unknown
  `offset`/`page_size` fields in examples) and core authentication-placement
  move diagnostics (`E0507`), plus inactive-code warnings. This is an
  automated rust-analyzer baseline failure, not interactive editor
  acceptance.
- `git diff --check` and the pre-edit `git diff --exit-code`: exit 0.
- The first wildcard source-discovery command exited 1 because PowerShell did
  not expand `crates/*/Cargo.toml` or `tools/*/Cargo.toml`; the corrected
  `rg -g Cargo.toml` command exited 0 and confirmed the public
  `dangerous-dev-tools` feature plus core's optional normal dependency on
  `concord-test-support`.

Unexecuted checks:

- `actionlint` is unavailable on PATH, so no actionlint result is claimed.
- `git remote -v` was requested but rejected by the execution environment's
  remote-access policy; remotes remain uninspected.
- No graphical editor session was available. Completion, hover, navigation,
  rename-dependency behavior, and visual diagnostic presentation are
  unexecuted.
- No GitHub Actions run or remote Linux/macOS/Windows platform lane was
  executed.
- No registry publication, package-name ownership check, license decision, or
  repository metadata decision was attempted.

Security/redaction review:

- C0 made no runtime or public API change, so no secret, protected URL,
  authentication value, rate key, or native transport surface was added to
  errors or evidence.
- Existing all-feature core and façade security/lifecycle owners passed.
- The recorded failures contain structural names and compiler diagnostics
  only; no secret or protected endpoint value is included.

Performance impact:
None. C0 changed documentation/evidence only and ran no new performance
measurement.

Deviations:
None. The audit's workflow-absence statement is recorded as an observed
archive/current-checkout discrepancy, not treated as a design deviation.

Remaining blockers:

- C1–C5: finite runtime construction, capped formatting, rate-limit caps,
  OAuth2 bounds, endpoint identity, timeout naming, and macro normalization.
- C6: remove public `dangerous-dev-tools`, remove the normal product
  test-support edge, and prove local package resolution.
- C7: establish executable locked validation, workflow lint/policy checks,
  passing MSRV semantic diagnostics, automated rust-analyzer correctness, and
  truthful docs/evidence.
- C8: complete the local certification matrix and retain any genuinely
  external editor/platform/legal/registry blockers with owners and next
  actions.
