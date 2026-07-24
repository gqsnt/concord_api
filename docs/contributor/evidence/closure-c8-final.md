# Closure C8 final certification

Phase:
Closure C8 — Run the final local proof matrix and hand off external release
checks honestly.

Started from commit:
`e76d515ec1f2a656dcc7011afc59a72aa86f7979`
(`ci: make Concord closure validation executable`) on branch `master`.

Finished at commit:
The C8 commit containing this evidence, with message
`release: close remaining Concord v2 refactor gaps`. A Git commit cannot
embed its own content-derived identifier; the exact C8 hash is therefore
owned by the required post-commit `git rev-parse HEAD` result and the final
handoff.

Files changed:

- `CHANGELOG.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `docs/contributor/evidence/closure-c8-final.md`.

Design decisions:

- C8 changes release truth and evidence only. It does not change product
  source, manifests, fixtures, snapshots, workflow behavior, or runtime
  semantics.
- The complete required matrix was run on the committed C7 tree before C8
  evidence edits. `just verify-release-local` was directly observed from a
  clean tree and exited 0. Every command in the explicit matrix was then run
  separately so focused owners have attributable results.
- Stable Rust remains the exact Trybuild snapshot owner. Rust 1.85 remains
  the passing semantic JSON-diagnostic owner. No snapshot was updated.
- Performance results remain observational. The required unlocked smoke was
  executed exactly, and a locked repeat is the recorded comparison. A
  100-iteration pagination outlier was investigated with three identical
  repeats and source history rather than converted into a new threshold.
- The closure status is **Implementation and local validation complete;
  external release checks pending.** Interactive editor acceptance, remote
  Actions/platform execution, and release/legal/name decisions remain
  explicit owner actions.
- Nothing was pushed, published, deployed, or changed in a remote.

Audit findings closed:

| Finding | Final classification | Proof owner |
|---|---|---|
| F-01 | Fixed and proven | C2 incremental runtime budgets, error latch, first-excess tests, and final core bounds/lifecycle plus façade runtime/planning owners |
| F-02 | Fixed and proven | C3 fallible bounded rate structures, governor preparation cap, and final core/extras/workspace owners |
| F-03 | Fixed in source and locally proven | Pinned executable workflow, repository policy, and Actionlint; remote workflow execution remains an F-04 external release check |
| F-04 | Local implementation/certification proven; externally blocked | Complete local matrix and package proof pass; editor, remote platforms/Actions, and release-owner decisions remain |
| F-05 | Fixed and proven | Dependency policy and packaged normalized manifests contain no normal/build product edge to test support |
| F-06 | Fixed and proven | Stable exact UI and both Rust 1.85 semantic diagnostic owners pass |
| F-07 | Externally blocked | Automated analysis is recorded; a graphical nine-row editor session remains owned by the repository maintainer |
| F-08 | Fixed and proven | Bounded OAuth2 configuration/encoding and provider-limit owners pass in core, extras, façade, and workspace suites |
| F-09 | Fixed and proven | Infallible endpoint conversion is absent; stable exact UI and public construction owners pass |
| F-10 | Fixed and proven | Macro semantic normalization and emit source-policy owner pass; direct inventory finds no emit-to-syntax import |
| F-11 | Fixed and proven | Runtime-only timeout rename is present in source, migration, changelog, planning, and public-type owners |
| F-12 | Fixed and proven | Public development feature is absent from product topology; package/dependency policy passes |
| F-13 | Fixed and proven | Locked, separated Just/CI scopes and parsed policy owners pass |
| F-14 | Fixed for maintained source/evidence | Documentation generation, evidence links, anchors, workflow policy, and final truth review pass |
| F-15 | Fixed and proven | README states unpublished status before executable checkout syntax; documentation policy passes |
| F-16 | Fixed and proven | Core/public exact owners and mechanically checked limits reference pass |
| F-17 | Fixed and proven | Obsolete core bridges are absent; generated/direct-path and implementation-feature policy owners pass |
| F-18 | Fixed and proven | Shared capped formatter covers Concord-owned runtime/generated/public-head capture; bounds and redaction owners pass |

Tests added/changed:
None in C8. This phase reruns the complete existing proof system without
changing a test, fixture, expectation, or Trybuild snapshot.

Commands executed:

The required aggregate command was run directly from a clean tree:

```text
just verify-release-local
```

The complete explicit matrix was then run:

```text
cargo fmt --all -- --check
cargo metadata --format-version 1 --all-features --locked
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings

cargo test --workspace --locked
cargo test --workspace --all-features --locked

cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord --test public_extensions --all-features --locked
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord --test public_compile --no-default-features --locked
cargo test -p concord --test rustdoc --all-features --locked

cargo test -p concord-macros --lib --locked
cargo test -p concord --test ui --no-default-features --locked
cargo test -p concord --test ui --all-features --locked
cargo test -p concord-core --test ui --no-default-features --locked

cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
cargo +1.85.0 test -p concord-core --lib --no-default-features --locked
cargo +1.85.0 test -p concord-extras --all-features --locked
cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked

cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- docs
cargo run --locked -p concord-xtask -- package

cargo test -p concord-examples --all-features --locked
cargo test --workspace --doc --locked
cargo test --workspace --doc --all-features --locked
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings

cargo deny check
actionlint .github/workflows/ci.yml

cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo bench -p concord-perf --bench concord_overhead -- --smoke
cargo run --locked -p concord-perf --bin expansion_scale

git diff --check
git status --short
```

Additional directly executed certification/investigation commands:

```text
cargo bench -p concord-perf --bench concord_overhead --locked -- --smoke
cargo run --locked -p concord-perf --bin expansion_scale
$env:CONCORD_BENCH_ITERATIONS='100'; cargo bench -p concord-perf --bench concord_overhead --locked
cargo run --locked -p concord-xtask -- policy
cargo test -p concord --test dependency_policy --all-features --locked
rust-analyzer --version
rust-analyzer -q analysis-stats . --run-all-ide-things
rust-analyzer -q diagnostics .
```

The 100-iteration benchmark command was repeated three times after one
pagination outlier. Direct `rg` inventories checked production emit/syntax
imports and macro extras names; a PowerShell token inventory checked the
required migration/changelog terms. `git diff` reviewed benchmark and
pagination changes since the closure start. `Get-Content`, `rg`, Git status,
Git history, process inspection, and the mandated plan/evidence reads were
also used for evidence reconciliation.

After creating this evidence and updating the ledger/release status, the
following commands were repeated:

```text
cargo fmt --all -- --check
cargo run --locked -p concord-xtask -- docs
cargo run --locked -p concord-xtask -- policy
git diff --check
```

One earlier `just verify-release-local` invocation lost its controlling tool
session after interruption. Process inspection proved that it continued, but
its exit code was unavailable, so no result was inferred from it. It was
rerun from the beginning in the directly observed invocation above. One
ancillary PowerShell source-diff command failed before execution because
`"$name:"` was parsed as an invalid variable reference; the corrected
`${name}` form exited 0 and supplied the recorded review.

Exact results:

- Directly observed `just verify-release-local`: exit 0 in 599.5 seconds.
- Formatting, locked metadata, default/no-default/all-feature all-target
  checks, and warning-denied stable Clippy: exit 0.
- Default and all-feature workspace suites: exit 0.
- Focused core owners: lifecycle 6 passed, security 3 passed, bounds 12
  passed.
- Focused façade owners: generated surface 1 passed, public extensions 3,
  endpoint planning 5, runtime endpoint lifecycle 9, public compile 7, and
  rendered Rustdoc 2.
- Macro library: 67 passed.
- Stable exact UI: façade no-default 6 harnesses, façade all-feature 8, and
  core no-default 3; all exited 0 and no `.stderr` changed.
- Rust 1.85: locked no-default all-target workspace check exited 0; core
  library had 285 passed; extras had 46 unit tests plus all integration and
  doctest owners pass; façade and core semantic diagnostic owners each passed
  their five separately compiled failure cases.
- Fixture driver: exit 0; locked fixture metadata and all 7 serialized public
  compile cases passed.
- Documentation owner: exit 0; 140 local file links and 2 local anchors
  passed, with 0 external Markdown URLs syntax-counted and none fetched.
- Package owner: exit 0 in core, macros, extras, façade order. It created and
  inspected archives, normalized manifests, and locked extracted all-feature
  library builds. Archive observations were core 174 files/254.1 KiB
  compressed, macros 36/68.8 KiB, extras 22/35.7 KiB, and façade 140/91.0
  KiB. Cargo warned that license, documentation/homepage, and repository
  metadata are absent.
- Examples: construction 1, local runtime 7, and Riot contracts 3 passed;
  every selected example target and doctest passed.
- Default doctests: core 2 and extras 1 passed. All-feature doctests: core 4
  and extras 1 passed. Warning-denied Rustdoc for façade, core, and extras
  each exited 0.
- `cargo deny check`: exit 0. The reviewed duplicate warnings remain
  `core-foundation` 0.9.4/0.10.1 and `windows-sys` 0.52.0/0.61.2.
- Actionlint 1.7.12: exit 0 for `.github/workflows/ci.yml`.
- Repository/Just/workflow policy and focused dependency policy: exit 0.
  Direct inventories found zero production emit-to-syntax imports and zero
  macro extras-name matches. Migration/changelog contain rate constructor
  fallibility, endpoint conversion removal, timeout rename, removed
  development feature, OAuth2 limits, and exact structural-limit guidance.
- Benchmark no-run, required unlocked smoke, locked smoke, 100-iteration
  investigation, and both expansion runs: exit 0. Every benchmark execution
  audited 16/16 named categories.
- Post-evidence formatting, documentation, repository policy, and
  `git diff --check`: exit 0. Documentation still checked 140 local file
  links, 2 local anchors, and 0 external Markdown URLs without fetching a
  network target.
- Pre-evidence `git diff --check` and `git status --short`: exit 0 with no
  status output. Post-commit clean-tree commands are owned by the final
  handoff because this evidence must exist before its containing commit.

Rust-analyzer result:

- `rust-analyzer 1.97.1 (8bab26f4 2026-07-14)`.
- `analysis-stats --run-all-ide-things`: exit 0 over 50 crates, 339 workspace
  item trees, 75,020 workspace lines, and 250 IDE files.
- `diagnostics`: exit 1. It retained 118 unresolved macro calls, all naming
  `emit_generated_raw_response_method`, with E0282=209, E0432=52, E0277=28,
  and E0599=5 cascades. It contained zero `fixtures/msrv` paths, zero old
  pagination-field errors, and zero old authentication `HeaderName` move
  errors.
- Compiler, Clippy, exact UI, semantic UI, fixture, and workspace owners all
  pass this generated surface. The CLI result remains a documented nested
  façade-macro analyzer limitation, not a passing diagnostic claim.

Unexecuted checks:

- No graphical editor was available. All nine rows in
  `docs/contributor/evidence/rust-analyzer-acceptance.md` remain pending.
- No GitHub Actions workflow ran. No independent remote Linux, Windows, or
  macOS lane ran because nothing was pushed.
- No external Markdown URL was fetched; the maintained Markdown inventory
  currently contains zero syntax-counted external URL targets.
- No registry ownership, public package-name approval, license choice,
  copyright decision, or canonical repository/homepage decision was made.
- No package was published and no branch was pushed.

Security/redaction review:

- Final core bounds/security/lifecycle, façade planning/runtime lifecycle,
  full default/all-feature workspaces, exact/semantic UI, package, fixture,
  and documentation owners all pass.
- Runtime endpoint construction remains incrementally budgeted and latched;
  rate plans and limiter preparation remain hard-capped; OAuth2 raw and
  encoded inputs remain checked before allocation/transport; every
  Concord-owned formatted capture remains capped.
- Tests retain first-excess withholding, inert construction/planning,
  immutable client snapshots, one executor, private native transport and
  authentication material, one additional authentication recovery attempt,
  sequential cycle-detecting all-or-error pagination, exactly-once
  completion, cancellation without detached correctness work, and sealed
  acquisition.
- Error, diagnostic, hook, completion, plan, description, `Debug`, and
  `Display` owners remain seeded and redacted. C8 logs/evidence contain no
  credential, authorization value, rate key, protected URL, or native
  transport value.
- Deterministic tests and performance probes used local support only; no
  deterministic test made an external network request.

Performance impact:

The locked one-iteration smoke compared with the Phase 14 Windows/MSVC smoke
is below. These are noisy observations, not thresholds.

| Category | Phase 14 | C8 | Observed change |
|---|---:|---:|---:|
| Core client construction | 181.2 µs | 144.9 µs | -20.0% |
| Generated endpoint construction | 3.0 µs | 1.6 µs | -46.7% |
| Request binding | 1.1 µs | 0.2 µs | -81.8% |
| Safe planning | 19.2 µs | 22.8 µs | +18.8% |
| Public-head preparation | 18.6 µs | 14.6 µs | -21.5% |
| Codec | 2.5 µs | 1.5 µs | -40.0% |
| Body-limit polling | 5.2693 ms | 2.8037 ms | -46.8% |
| Direct execution | 2.0590 ms | 2.5609 ms | +24.4% |
| Explicit execution | 2.4368 ms | 2.5156 ms | +3.2% |
| Runtime execution | 2.6529 ms | 2.6644 ms | +0.4% |
| Credential ready path | 24.8 µs | 23.4 µs | -5.6% |
| Limiter ready path | 2.5122 ms | 2.5378 ms | +1.0% |
| Pagination | 2.6050 ms | 2.8359 ms | +8.9% |
| Stream wrapping | 0.6 µs | 0.8 µs | +33.3% |
| Generic specification selection | 4.7 µs | 5.9 µs | +25.5% |
| Record framing | 0.6 µs | 0.5 µs | -16.7% |

The first 100-iteration C8 pagination observation was 84.5595 ms versus
Phase 14's 26.9209 ms. Three immediate identical repeats measured 22.5982,
21.5448, and 46.6867 ms. Phase 14 direct/explicit/runtime totals were
13.0386/12.5988/14.9175 ms; the repeats remained in
11.0341–13.5758/11.4977–13.8199/11.9095–13.3855 ms respectively. Source
history contains no closure-era production pagination change, only test
updates. The isolated outlier is therefore recorded as variable Windows
loopback scheduling, not a repeatable product regression.

Warm expansion observations and Phase 14 text owners:

| Endpoints | Phase 14 lines/chars/fragments/declarations | C8 lines/chars/fragments/declarations | Phase 14 → C8 warm time |
|---:|---:|---:|---:|
| 1 | 560 / 25,963 / 1,783 / 74 | 571 / 26,375 / 1,805 / 77 | 426 → 495 ms |
| 10 | 3,242 / 164,203 / 10,819 / 425 | 3,352 / 168,323 / 11,039 / 455 | 463 → 498 ms |
| 100 | 30,422 / 1,553,263 / 101,539 / 3,935 | 31,522 / 1,594,463 / 103,739 / 4,235 | 701 → 737 ms |
| 1,000 | 302,222 / 15,454,663 / 1,008,739 / 39,035 | 313,222 / 15,866,663 / 1,030,739 / 42,035 | 3,617 → 3,413 ms |

The exact linear increase is 11 lines, 412 characters, 22 fragments, and 3
declaration-shaped lines per endpoint. It is explained by C6's intentional
always-generated, doc-hidden, token-sealed benchmark conversion for each
named request wrapper. It preserves the performance probe without a product
test-support edge or public feature. Warm expansion time ranged from about 6%
faster to 16% slower across the four one-shot sizes and shows no monotonic
material regression.

Deviations:
None. The required unlocked smoke command was run exactly; the additional
locked repeat owns the reproducible comparison. The non-green
rust-analyzer CLI diagnostic and every external release gate remain explicit.

Remaining blockers:

- Owner: repository maintainer with a graphical Rust editor. Run all nine
  rows in `rust-analyzer-acceptance.md` with the pinned project toolchain and
  record editor/extension versions, feature configuration, and results.
- Owner: repository maintainer with push and GitHub Actions authority. Push
  an approved branch or open a pull request, run the pinned workflow, and
  record all required jobs plus Linux/Windows/macOS results.
- Owner: repository maintainer and, if independently reproduced, upstream
  rust-analyzer. Recheck the 118 nested façade-macro diagnostics in the newest
  approved editor/analyzer and file an upstream issue if they persist.
- Owner: release/legal owner. Approve license/copyright and canonical
  repository/homepage metadata, approve public package names and registry
  ownership, and explicitly decide whether `publish = false` may change.
