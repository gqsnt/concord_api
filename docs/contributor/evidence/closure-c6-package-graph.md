# Closure C6 package graph

Phase:
Closure C6 — Remove test-support leakage and make packaging technically
coherent.

Started from commit:
`38992febea6392f44071c5a12edc150449b8aa11`
(`refactor: finish Concord semantic API boundaries`) on branch `master`.

Finished at commit:
The C6 commit containing this evidence, with message
`build: remove test support from Concord product graph`. Its exact hash will
be backfilled in the closure ledger by C7.

Files changed:

- `CHANGELOG.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `MIGRATION.md`;
- `crates/concord-core/Cargo.toml`;
- `crates/concord-core/src/__development.rs` (removed);
- `crates/concord-core/src/__benchmark.rs` (added);
- `crates/concord-core/src/auth/placement.rs`;
- `crates/concord-core/src/execute/mod.rs`;
- `crates/concord-core/src/http/application.rs`;
- `crates/concord-core/src/http/build.rs`;
- `crates/concord-core/src/http/mod.rs`;
- `crates/concord-core/src/http/provider.rs`;
- `crates/concord-core/src/lib.rs`;
- `crates/concord-core/tests/configuration/dangerous_features.rs`;
- `crates/concord-core/tests/ui.rs`;
- `crates/concord-core/tests/ui/fail/benchmark_conversion_token_private.rs`;
- `crates/concord-core/tests/ui/fail/benchmark_conversion_token_private.stderr`;
- the five obsolete development-feature `.rs`/`.stderr` pairs under
  `crates/concord-core/tests/ui/fail/` (removed);
- `crates/concord-examples/src/lib.rs`;
- `crates/concord-macros/src/emit/requests.rs`;
- `crates/concord/Cargo.toml`;
- `crates/concord/src/lib.rs`;
- `crates/concord/tests/dependency_policy.rs`;
- `crates/concord/tests/public_compile.rs`;
- `crates/concord/tests/rustdoc.rs`;
- `crates/concord/tests/ui.rs`;
- `crates/concord/tests/ui/fail/capabilities/no_content_request_spec.stderr`;
- `crates/concord/tests/ui/fail/capabilities/response_only_request_spec.stderr`;
- `docs/contributor/evidence/closure-c5-api-macro-cleanup.md`;
- `docs/contributor/evidence/closure-c6-package-graph.md`;
- `docs/reference/dangerous.md`;
- `docs/reference/features.md`;
- `fixtures/Cargo.lock`;
- `fixtures/features/Cargo.toml`;
- `fixtures/features/src/bin/development_absent.rs` (removed);
- `tools/perf/Cargo.toml`;
- `tools/perf/README.md`;
- `tools/perf/benches/concord_overhead.rs`;
- `tools/xtask/src/main.rs`.

Design decisions:

- The public `dangerous-dev-tools` feature is removed from core, the façade,
  fixtures, documentation, and generated assertions. Core and façade retain
  `concord-test-support` only as a development dependency.
- Deterministic application/provider channels now compile only under
  `cfg(test)`. Ordinary product builds always use the existing private managed
  Reqwest boundary; no second transport or executor was introduced.
- Operation-exact performance probes live in always-available but
  `#[doc(hidden)]` core/façade `__benchmark` modules. The bridge depends only
  on core and exposes no test-support or native-transport type.
- Generated named request wrappers implement one hidden conversion. Its
  capability token has a private field, so downstream code cannot directly
  invoke the conversion. The benchmark helpers create the token internally,
  require the ordinary sealed `EndpointSpec` bound, and call the existing
  `prepare_operation` path.
- The generated conversion implementation deliberately carries no endpoint
  `where` clause. The helper owns that semantic bound, preventing the hidden
  benchmark bridge from adding duplicate diagnostics to intentionally invalid
  DSL declarations.
- `concord-perf` imports `concord-test-support` directly for loopback
  infrastructure and imports product behavior only through the façade. All 16
  prior benchmark categories remain present.
- `xtask package` derives the product version from `[workspace.package]` and
  packages in publication order: core, macros, extras, façade. Workspace
  source patches make unpublished siblings resolvable during archive
  preparation. Each extracted normalized manifest is then checked separately
  against previously extracted packaged siblings with `--lib --all-features
  --locked`.
- The package verifier requires the archive, normalized `Cargo.toml`, and
  `Cargo.toml.orig`; exact name/version and `publish = false`; exact
  registry-version sibling dependencies; no normal test-support dependency;
  and no path entry in a normalized dependency section.
- Local patches resolve dev-only `concord-test-support` without treating it as
  a product package. They do not alter the normalized archive dependency
  facts.
- The fixture driver serializes `public_compile` with
  `--test-threads=1`, preventing Windows cases from racing on shared
  executables.

Audit findings closed:

- F-05 is closed: no product package has a normal or build dependency on
  `concord-test-support`; core and façade use it only as a dev dependency.
- F-12 is closed: the public `dangerous-dev-tools` feature and all feature-only
  proofs/documentation are removed.
- The package portion of F-04 is closed: all four intended product archives
  and their normalized manifests are locally verifiable in dependency order.
  Final release certification remains C8.
- The tooling portion of F-17 is closed: the former public development bridge
  and generated development helper are gone; the narrow benchmark owner is
  explicit and transport-free.

Tests added/changed:

- Added a stable exact core Trybuild owner proving
  `GeneratedRequestToken` cannot be constructed outside core. Its E0451
  snapshot was inspected directly.
- Macro unit coverage requires the hidden generated trait, token, and
  conversion and rejects the retired development method/macro.
- Dependency policy now proves no normal/build product support edge, dev-only
  core/façade ownership, direct benchmark ownership, façade-only benchmark
  product imports, façade-only examples, and the existing extras-agnostic
  macro direction.
- Rendered Rustdoc checks prove `__benchmark`, `__private`, and
  `__development` are absent from ordinary core and façade indexes.
- Removed development-feature configuration, fixture, and UI owners. The two
  changed façade exact snapshots were inspected individually and remove only
  diagnostics formerly emitted by the deleted generated development method.
- Fixture metadata/lock ownership reflects the removed feature and normal
  support edge.
- The package verifier tests every extracted product library with all features
  and locked resolution after inspecting the normalized manifest.

Commands executed:

```text
cargo fmt --all -- --check
cargo check -p concord-xtask --all-targets --locked
cargo test -p concord-core --test ui --no-default-features --locked
cargo test -p concord --test ui --no-default-features --locked
cargo run --locked -p concord-xtask -- docs
git diff --check
cargo metadata --format-version 1 --all-features --locked | Out-Null
cargo test -p concord --test dependency_policy --all-features --locked
cargo test -p concord-core --all-features --locked
cargo test -p concord --all-features --locked -- --test-threads=1
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo bench -p concord-perf --bench concord_overhead -- --smoke
cargo bench -p concord-perf --bench concord_overhead --locked -- --smoke
cargo run --locked -p concord-xtask -- fixtures
cargo run --locked -p concord-xtask -- package
cargo package --locked -p concord-core
cargo package --locked -p concord-macros
cargo package --locked -p concord-extras
cargo package --locked -p concord
cargo package --locked --allow-dirty -p concord-core
cargo package --locked --allow-dirty -p concord-macros
cargo package --locked --allow-dirty -p concord-extras
cargo package --locked --allow-dirty -p concord
cargo +stable clippy -p concord-core -p concord-extras -p concord-macros -p concord -p concord-perf -p concord-xtask --all-targets --all-features --locked -- -D warnings
cargo +1.85.0 check -p concord-core -p concord-extras -p concord-macros -p concord -p concord-perf -p concord-xtask --all-targets --all-features --locked
cargo test -p concord-macros --lib --locked
```

Read-only source review also used `rg`, `Get-Content`, `git diff`,
`git status --short`, and direct inspection of the generated package
manifests and changed exact snapshots.

Exact results:

- Final formatting, xtask compilation, documentation ownership, metadata, and
  `git diff --check` exited 0.
- Core no-default exact UI exited 0: all 3 harness owners passed, including
  the new private-token E0451 case and every existing source/privacy owner.
- Façade no-default exact UI exited 0: all 6 harness owners passed. The
  all-feature exact owner also passed as part of the full façade suite with 8
  harness tests.
- Dependency policy exited 0 with 11 passed, including
  `product_graph_keeps_test_support_dev_only_and_benchmark_owned`.
- Full core exited 0: 360 unit tests, 12 bounds, 4 configuration, 6 lifecycle,
  25 public-type, 3 security, 18 transport, 1 exact-UI harness, and 4 doctests
  passed.
- The full all-feature façade suite was deliberately serialized and exited 0.
  Every integration owner passed, including 7 public Cargo-graph cases, 8
  stable exact UI harnesses, 2 rendered Rustdoc tests, and doctests.
- Macro unit tests exited 0 with 67 passed.
- Warning-denied stable Clippy for core, extras, macros, façade, performance,
  and xtask exited 0. Rust 1.97's localized MSVC `linker_messages` warning is
  explicitly exempt from `-D warnings`.
- Rust 1.85 all-target/all-feature compilation for the same six packages
  exited 0.
- Optimized benchmark compilation exited 0 and produced
  `concord_overhead-79d95e34a4c3d07f.exe`.
- The first smoke invocation omitted `--locked`; it passed all 16 categories
  but is not the authoritative measurement. The locked repeat exited 0 and
  reported:
  core client construction 147.3 µs; generated endpoint construction 2.5 µs;
  request binding 200 ns; safe public planning 22.3 µs; public-head
  preparation 14 µs; codec 1.6 µs; body-limit polling 4.4495 ms; direct
  execution 2.5859 ms; explicit generated execution 2.6882 ms; runtime
  endpoint execution 658.6 µs; credential-ready preparation/finalization
  21.5 µs; limiter acquisition 2.6624 ms; pagination 2.9061 ms; stream
  wrapping 600 ns; generic specification selection 5.4 µs; record framing
  600 ns. The operation audit reported 16/16.
- The serialized fixture driver exited 0: locked fixture metadata passed and
  all 7 no-default public-compile cases passed.
- `xtask package` exited 0. It produced and inspected all four archives in the
  required order and separately compiled every extracted normalized library
  with all features and locked resolution. Cargo warned that license,
  documentation/homepage, and repository metadata are absent.
- Ordinary direct package commands for dirty core, macros, and façade exited
  1 before preparation and identified 13, 1, and 8 dirty package files,
  respectively. Extras had no dirty source and exited 1 because unpublished
  `concord-core = 0.1.0` is absent from crates.io.
- With `--allow-dirty`, macros packaged and verified successfully. Core exited
  1 because Cargo resolves its unpublished dev-only
  `concord-test-support`; extras and façade exited 1 on unpublished product
  siblings. These ordinary-registry failures are distinct from the passing
  local patched package proof and do not represent a product-graph cycle.
- A source inventory found retired feature/bridge names only in migration and
  changelog history or negative assertions. Product manifests contain no
  `dangerous-dev-tools`; support remains dev-only in core/façade and direct in
  unpublished tools/fixtures/examples as intended.
- One ancillary `rg` expression was malformed by PowerShell quoting and was
  rerun with literal quoting. An ad hoc normalized-manifest search first had a
  PowerShell interpolation error and then matched ordinary `[lib]`/`[[test]]`
  target paths; the section-aware xtask verifier is the authoritative
  dependency-path check and passed.

Unexecuted checks:

- A pre-C6 benchmark replay in an alternate worktree was not executed:
  alternate-worktree inspection was rejected by the execution environment
  pending separate user authorization. No before/after performance claim is
  made from remembered output.
- No graphical editor or manual rust-analyzer acceptance session ran in C6.
- No remote GitHub Actions or independent Linux/macOS lane ran.
- No registry publication, registry ownership check, package-name approval,
  license choice, or repository/homepage metadata decision was made.
- No product package was pushed or published.

Security/redaction review:

- Normal product compilation has no deterministic channel or test-support
  type. Test transport is reachable only under core's test cfg, while
  performance loopback ownership stays in the unpublished benchmark package.
- The hidden bridge performs ordinary public-head and ready-credential
  preparation only. It exposes aggregate operation/count observations, never
  native request/response objects, authentication material, headers, URLs,
  rate keys, provider channels, or test-support values.
- The generated carrier conversion requires a token whose field is private.
  Its exact compile-fail owner passed. The bridge neither sends HTTP nor adds
  an alternate executor.
- Full core security, lifecycle, bounds, transport, stable exact UI, and full
  façade lifecycle owners passed. Authentication recovery, first-excess
  withholding, completion, cancellation, sealed acquisition, and redaction
  paths are unchanged.
- Package/test output and this evidence contain no secret, credential, rate
  key, authorization value, or protected endpoint.

Performance impact:

- All 16 operation-exact benchmark categories compile and execute through the
  façade. Public-head and ready-credential probes use the same production
  preparation operations as before, with only a zero-sized private token at
  the generated conversion boundary.
- The locked one-iteration smoke numbers above are a noisy local observation,
  not a threshold or regression claim. No comparable pre-C6 run was available
  after the required context recovery, so no numeric improvement/regression is
  claimed.

Deviations:
None. The local two-stage archive plus extracted-manifest verification is the
closure plan's selected sibling-patch/source-replacement strategy.

Remaining blockers:

- None for C6 implementation or technical package verification.
- Owner/release action remains required to choose legal metadata, approve
  registry package names/ownership, and decide repository/homepage metadata.
  `publish = false` remains intentionally unchanged.
- C7 owns executable CI, action policy, locked validation scopes, passing MSRV
  semantic diagnostics, automated rust-analyzer checks, and documentation
  truth.
