# Phase 0 Integrated Conformance Report

## 1. Verdict

`PHASE_0_COMPLETE_WITH_LIMITATIONS`

All accepted Phase 0 source units and every non-checker validation required by PR 0.7 passed at committed baseline `ca578db25acace2b9b95fee5f0df04157a8cb786`. The specification checker and its negative tests were explicitly excluded by user instruction. Because that gate was part of the original Phase 0 definition, this report does not claim `PHASE_0_COMPLETE`.

## 2. Workspace Identity

- Repository root: `F:/projects/concord_api`
- Branch: `master`
- PR 0.6 baseline: `ca578db25acace2b9b95fee5f0df04157a8cb786`
- PR 0.6 subject: `phase0: prove single typed executor parity`
- Baseline parent: `2a3fac634d6e988a29a566bda1049ecdb4ac31fe`
- Initial state: clean tracked and untracked working tree; `git status --branch --short` was `## master`
- Rust: `rustc 1.85.0 (4d91de4e4 2025-02-17)`
- Cargo: `cargo 1.85.0 (d73d2caf9 2024-12-31)`
- Specification status: every normative document remains `Normative Implementation Candidate`.
- Validated PR 0.6 patch: `git_diff/phase0-pr-0.6-final.patch`, 42,744 bytes, SHA-256 `6f815eb515928fe9604ffc5dde2377e4298b8d784a909ebb81c9797096b938e5`; reverse apply check passed and its stat was exactly 5 files changed with 1,301 insertions.

The accepted history is a linear six-commit chain. `git rev-list --merges 17fde39b6497757a726c3168e3208c2640bf5586^..HEAD` returned no merge commit. No unreviewed commit exists between accepted units. Reviewed later-unit updates include shared manifests, `Cargo.lock`, `deny.toml`, and `concord_core/src/lib.rs`; these cross-unit integration changes were included in the source and commit audit.

## 3. Accepted Units and Commits

| Unit | Commit | Exact subject | Exact stat | Accepted limitation |
|---|---|---|---|---|
| PR 0.1 | `17fde39b6497757a726c3168e3208c2640bf5586` | `phase0: establish workspace and specification gates` | 17 files; 997 insertions; 7 deletions | Workspace and governance scaffold; checker is now user-excluded and specifications remain candidates. |
| PR 0.2 | `b7a44b0c3070e8a368f3f2a160712b9493f8d11f` | `phase0: prove macro core and scoped endpoint contracts` | 40 files; 2,679 insertions; 5 deletions | Generated construction and compile fixtures only; no request execution. |
| PR 0.3 | `677aa04748ee98ff15906863c057c617c3f01893` | `phase0: prove credential transition and finalization invariants` | 7 files; 2,647 insertions | Private reduced credential model; no production credential runtime or secrets. |
| PR 0.4 | `8705ff67894e9b40546574fafaf4c51a2013bd2f` | `phase0: verify reqwest and terminal response contracts` | 12 files; 2,784 insertions; 5 deletions | Local deterministic Reqwest probes; connection behavior is observation rather than guarantee. |
| PR 0.5 | `2a3fac634d6e988a29a566bda1049ecdb4ac31fe` | `0.5` | 6 files; 1,359 insertions | Private pagination model; logical URL canonicalization is supplied as an assumption. |
| PR 0.6 | `ca578db25acace2b9b95fee5f0df04157a8cb786` | `phase0: prove single typed executor parity` | 5 files; 1,301 insertions | Private typed executor; no production prepared-endpoint or managed Reqwest execution. |

The PR 0.5 subject differs from the recommended subject but is recorded without rewriting. PR 0.6 content exactly matches the validated patch identity.

## 4. Crates and Major Files

The root remains a virtual resolver-3 workspace with exactly `concord_core`, `concord_macros`, `concord_test_support`, and `concord_examples`. There is no root `[package]`, root `src/`, or root binary. `concord_macros` remains a proc-macro crate. `concord_test_support` and `concord_examples` remain unpublished; all four workspace crates currently declare `publish = false`.

Normal `concord_core` production dependencies contain only mandatory Reqwest 0.13.4 with default features disabled. HTTP/2, Rustls, system-proxy, Tokio, H2, HTTP, macros, and test-support dependencies used by probes are dev-only. There is no production dependency cycle; the core/macro relationship is dev-only in both directions.

Intentional retained proof modules are:

- PR 0.2 downstream macro fixtures and generated-surface tests under `concord_macros/tests/`;
- PR 0.3 private `concord_core/src/auth/`, gated by `#[cfg(test)]` at the crate root;
- PR 0.4 local integration probes in `concord_core/tests/` and unpublished `concord_test_support` helpers;
- PR 0.5 private `concord_core/src/pagination/`, gated by `#[cfg(test)]`;
- PR 0.6 private `concord_core/src/execute/`, gated by `#[cfg(test)]`.

No obsolete spike, duplicate production execution loop, generated output, certificate file, patch outside `git_diff/`, public transport/executor trait, runtime `Any`, public native Reqwest request/response, public pagination runtime, or public credential state was found. The fixture checks created `concord_macros/tests/fixtures/target/`; `cargo clean --manifest-path concord_macros/tests/fixtures/Cargo.toml` removed that generated output before documentation edits.

## 5. Integrated Commands

The original integrated matrix ran from the clean committed PR 0.6 baseline before evidence edits. The metadata command was rerun during the original evidence correction to reconcile the workspace metadata row. All listed commands exited 0.

### Workspace gates

| Exact command | Relevant result |
|---|---|
| `cargo fmt --all -- --check` | Passed; no output. |
| `cargo +1.85.0 check --workspace --all-targets` | Passed on Rust 1.85.0. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed with warnings denied. |
| `cargo test --workspace` | Passed: core unit 115; generated contract 2; HTTPS auth 6; Reqwest 13; status body 12; macro unit 4; compile harness 1; generated surface 3; other targets/doc-tests 0; 156 total tests. |
| `cargo check -p concord_core --lib --no-default-features` | Passed capability-minimal core library build. |
| `cargo check --workspace --no-default-features` | Passed all workspace members. |
| `cargo check --workspace --all-features` | Passed all declared features. |
| `cargo nextest run --workspace` | Passed 156/156 across 10 binaries; 0 skipped; retries configured to zero. |
| `cargo deny check` | Advisories, bans, licenses, and sources passed. Warnings only: duplicate `core-foundation` 0.9.4/0.10.1 and `windows-sys` 0.52.0/0.61.2. |
| `cargo doc --workspace --no-deps` | Passed; generated four workspace documentation outputs with no rustdoc warning. |
| `just release` | Passed the maintained release recipe: fmt, MSRV check, Clippy, workspace tests, no-default/all-feature checks, Nextest 156/156, Cargo Deny, rustdoc, and `git diff --check`. |
| `cargo metadata --no-deps --format-version 1` | Passed during PR 0.7 correction; reported four workspace packages and confirmed `concord_macros` normal dependencies are only proc-macro tooling while `concord_core` is dev-only. |

`cargo doc --workspace --no-deps` passed without warnings, but the maintained release command does not set `RUSTDOCFLAGS="-D warnings"`; therefore Rustdoc-warning failure enforcement is not proved.

### PR 0.2 fixtures and harnesses

| Exact command | Relevant result |
|---|---|
| `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/normal/Cargo.toml` | Passed normal downstream dependency. |
| `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/renamed_core/Cargo.toml` | Passed renamed core dependency. |
| `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/nested_module/Cargo.toml` | Passed nested expansion. |
| `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/repeated_invocation/Cargo.toml` | Passed repeated expansion. |
| `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/equal_name/Cargo.toml` | Passed equal-name scope/endpoint namespaces. |
| `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/minimal_bounds/Cargo.toml` | Passed non-Clone minimal-bound fixture. |
| `cargo test -p concord_macros --test compile` | 1/1 passed; six success fixtures and four exact expected-failure fixtures, including core mismatch and missing scope. |
| `cargo test -p concord_macros --test generated_surface` | 3/3 passed. |
| `cargo test -p concord_core --test generated_contract` | 2/2 passed. |

### PR 0.3 credential model

| Exact command | Relevant result |
|---|---|
| `cargo test -p concord_core auth_model` | 51/51 filter matches: 40 deterministic model tests plus 11 systematic tests; 64 core tests filtered out. |
| `cargo test -p concord_core auth_model_systematic` | 11/11 systematic tests; 36 candidate schedules, 33 explored, 3 pruned. |

The three pruned schedules are the permutations in which `Finalize` occurs before `Replace` in `auth_model_systematic_physical_source_replacement_bound_six_prunes_three`. They are causally outside that test's stated replacement-before-finalization scenario; no other systematic test prunes a schedule.

### PR 0.4 Reqwest probes

| Exact command | Relevant result |
|---|---|
| `cargo test -p concord_test_support` | Passed; 0 unit and 0 doc tests because the crate supplies bounded helpers to downstream integration tests. |
| `cargo +1.85.0 test --offline --locked -p concord_core --test reqwest_contract -- --test-threads=1` | 13/13 passed serially. |
| `cargo +1.85.0 test --offline --locked -p concord_core --test status_body_contract` | 12/12 passed. |
| `cargo +1.85.0 test --offline --locked -p concord_core --test https_auth_contract` | 6/6 passed. |
| `cargo +1.85.0 test --offline --locked -p concord_core --test reqwest_contract -- --test-threads=1 --nocapture` | Additional observation run: retry-off physical=1/connections=1; Reqwest-default physical=2/connections=1; hidden retry logical/hook/limiter/body factory/final auth/Reqwest call each=1 and physical=2; native timeout=133 ms; operation deadline=89 ms. |
| `cargo tree -p concord_core --no-default-features -e normal,build,features` | Normal graph contains mandatory Reqwest and its transitive stack; no probe-only direct dependencies. |
| `cargo tree -p concord_core --no-default-features -e normal,build,dev,features` | Dev graph additionally contains macro/test-support, H2, HTTP, Tokio, Rustls, and Reqwest `http2`, `rustls-no-provider`, and `system-proxy` features. |

HTTP/1.1 observations: two fully consumed small responses reused one connection; unread, known-oversized-abandoned, and unknown-oversized-abandoned responses were followed by a different connection. These are selected-baseline observations, not contractual guarantees.

### PR 0.5 pagination proof

| Exact command | Relevant result |
|---|---|
| `cargo test -p concord_core pagination_identity` | 24/24 passed: 20 identity tests plus 4 identity-bound tests matched by the filter. |
| `cargo test -p concord_core pagination_bounds` | 16/16 passed. |
| `cargo test -p concord_core pagination_controller` | 3/3 passed. |

### PR 0.6 executor proof

| Exact command | Relevant result |
|---|---|
| `cargo test -p concord_core executor_parity` | 3/3 passed. |
| `cargo test -p concord_core executor_typed` | 4/4 passed. |
| `cargo test -p concord_core executor_status` | 5/5 passed. |
| `cargo test -p concord_core executor_deadline` | 4/4 passed. |
| `cargo test -p concord_core generated_executor` | 1/1 passed. |

### Additional feature-isolation checks

| Exact command | Relevant result |
|---|---|
| `cargo check -p concord_core --lib --no-default-features --features dangerous-raw-response` | Passed independently. |
| `cargo check -p concord_core --lib --no-default-features --features dangerous-custom-retry` | Passed independently. |
| `cargo check -p concord_core --lib --no-default-features --features dangerous-dev-tools` | Passed independently. |

No required tool was unavailable. The specification checker was intentionally not invoked.

### Final staged validation

The documentation-only staged result passed: `cargo fmt --all -- --check`; `cargo +1.85.0 check --workspace --all-targets`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` (156 total); `cargo check -p concord_core --lib --no-default-features`; `cargo check --workspace --no-default-features`; `cargo check --workspace --all-features`; `cargo nextest run --workspace` (156/156); `cargo deny check`; `cargo doc --workspace --no-deps`; `just release`; `git diff --check`; `git diff --cached --check`. Every command exited 0. Cargo Deny repeated only the two duplicate-version warnings recorded above.

## 6. All 26 Completion Criteria

| # | Result | Source evidence | Validation evidence | Limitation or blocker |
|---:|---|---|---|---|
| 1 | PASS | Root `Cargo.toml` is a virtual resolver-3 workspace with exactly four members and no `[package]`. | Cargo metadata and all workspace gates passed. | None. |
| 2 | PASS | `rust-toolchain.toml` pins 1.85.0; workspace `rust-version = "1.85"`. | Rust/Cargo version checks and MSRV all-target check passed. | Public production surface remains intentionally small. |
| 3 | PASS | Accepted feature names/defaults match PR 0.1 and probe features are dev-only. | Default, no-default, all-feature, and three independent dangerous-feature checks passed. | Compilation evidence does not claim unimplemented runtime behavior. |
| 4 | PASS | Coverage plan and actual evidence remain separate CSV files. | Manual comparison and structural CSV validation passed. | Coverage plan was intentionally unchanged. |
| 5 | NOT PASSING — ACCEPTED LIMITATION | Normative index still defines checker governance requirements. | Checker files absent; no checker command was run and no checker evidence was added. | User explicitly excluded checker and negative tests; this prevents an unqualified complete verdict. |
| 6 | PASS | Macro crate uses caller-visible crate path resolution. | Renamed-core fixture and compile harness passed. | Construction surface only. |
| 7 | PASS | Generated aliases are invocation-isolated. | Nested and repeated fixtures passed. | Construction surface only. |
| 8 | PASS | Generated facade and explicit endpoint retain required scope values. | Generated-surface 3/3 and the missing-scope constructor-typing oracle passed. | The three `GEN-EXPL-002` rows prove only required outer-scope arguments, lexical ordering, and scope/endpoint separation; optional/defaulted scope controls are not implemented or proved. `GEN-EXPL-003` client-side no-inferred-scope behavior remains unproved. |
| 9 | PASS | Private auth model contains deterministic and exhaustive schedulers. | 40 deterministic tests and 11 systematic tests passed; 33 valid schedules explored. | Reduced model is not production auth. |
| 10 | PASS | Model has a post-limiter finalization transition. | Finalization deterministic tests and four systematic clear/replacement schedules passed. | Model-level proof only. |
| 11 | PASS | Challenge plan resolves defaults and rejects multiple challengers. | Focused challenge/default/attribution tests passed. | Model-level proof only. |
| 12 | PASS | Local test support bounds servers, TLS, proxy, gates, and waits. | Reqwest 13/13, status 12/12, HTTPS-auth 6/6 passed offline/locked. | Connection results are observations. |
| 13 | PASS WITH LIMITATION | Hardened probe builder uses `retry(never())`; raw Reqwest default behavior was observed separately. | Retry-off physical=1; Reqwest-default physical=2; hidden logical boundaries each=1. | The unimplemented `API-HTTP-002` safe retry opt-in API remains unproved; no row maps the raw comparison to it. |
| 14 | PASS WITH LIMITATION | Scheme gates precede credential/body materialization in private local proofs. | HTTPS-auth 6/6 passed, including localhost rejection. | `SEC-HTTP-009` has a lower-level provider-scheme proof; the `API-PHTTP-004` public `ProviderHttp` API remains unproved. |
| 15 | PASS | Terminal capture models known excess, unknown excess, read failure, deadline, and complete success. | Status-body 12/12 and executor status 5/5 passed. | Default 64 KiB value is normative future runtime behavior; Phase 0 probes use smaller test caps. |
| 16 | PASS | Canonical identity stores full encoded equality beside its digest. | Identity focus 24/24 including forced hash collision and cycles. | URL canonicalization is assumed input. |
| 17 | PASS WITH LIMITATION | Private model uses checked page/item/identity counters and pre-side-effect failures. | Bounds focus 16/16 passed. | The cited internal page-ceiling test does not prove requested-limit preflight; `BOUND-PAGE-007` and `EXEC-PAGE-010` remain evidence gaps. |
| 18 | PASS | Custom controller trait has no capacity hint and built-in short-page logic is sealed/private. | Controller focus 3/3 and public grep audit passed. | Production pagination API is future work. |
| 19 | PASS — PHASE 0 PROOF | One generic `execute_typed` accepts both generated and prepared operations. | Executor parity 3/3 and typed 4/4 passed. | Private proof; no production prepared endpoint execution. |
| 20 | PASS | Production crate exports no transport/executor trait; test modules are crate-private and cfg-gated. | Required greps and rustdoc passed; `Any` appeared only as a forbidden test string. | Doc-hidden generated ABI remains intentionally public. |
| 21 | PASS WITH LIMITATION | Maintained release recipe covers formatting, MSRV, Clippy, tests, features, deny, docs, and diff check. | Every integrated and final command exited 0; Clippy ran with `-D warnings`, and `cargo doc --workspace --no-deps` passed without warnings. | The maintained release command does not set `RUSTDOCFLAGS="-D warnings"`; therefore Rustdoc-warning failure enforcement under `CONF-CI-001` is not proved. Checker remains intentionally excluded. |
| 22 | PASS | Actual catalog contains only `passing` rows with empty waivers and committed baseline hash. | Corrected standard-library CSV validation found 57 rows, 57 unique IDs, existing paths, nonempty tests/configurations, report-field parity, and no invalid field. | Rows explicitly scope private models; no checker row or `CONF-CI-001` row exists. |
| 23 | PASS | Baseline `git status --short` and `git diff HEAD` were empty before evidence edits. | Entry commands recorded clean `master`. | Fixture build output was generated later and cleaned before edits. |
| 24 | PASS | `docs/conformance/PHASE_0_REPORT.md` exists with required sections. | Markdown diff and final diff checks passed. | Report records limitations instead of converting them to waivers. |
| 25 | PASS | All nine normative files retain `Normative Implementation Candidate`. | Repository-wide status search confirmed no normative document is release-final. | Candidate status is not release conformance. |
| 26 | PASS WITH LIMITATIONS | All non-checker Phase 0 gates passed. | Integrated and focused suites passed. | Exact verdict is `PHASE_0_COMPLETE_WITH_LIMITATIONS`. |

## 7. Conformance Evidence

The corrected catalog contains 57 added rows with 57 unique evidence IDs, all verified against committed source `ca578db25acace2b9b95fee5f0df04157a8cb786`. Original rerun evidence retains timestamp `2026-07-16T21:59:10Z`; the workspace-metadata correction rerun is timestamped `2026-07-16T22:25:47Z`. Evidence IDs and their exact oracles are:

| Evidence ID | Requirement | Type | Exact path | Exact test or command | Configuration and oracle limitation |
|---|---|---|---|---|---|
| P0-WORK-001 | COMPAT-RUST-002 | msrv | `rust-toolchain.toml` | `cargo +1.85.0 check --workspace --all-targets` | Rust 1.85.0 workspace all targets. |
| P0-WORK-002 | COMPAT-CRATE-001 | manual-review | `Cargo.toml` | `cargo metadata --no-deps --format-version 1` | Virtual four-package workspace; macro normal dependencies exclude runtime-only crates; core is dev-only. |
| P0-WORK-003 | FEAT-CORE-001 | feature-boundary | `concord_core/Cargo.toml` | `cargo +1.85.0 check --workspace --all-targets` | Declared default feature configuration. |
| P0-WORK-004 | FEAT-CORE-002 | feature-boundary | `concord_core/Cargo.toml` | `cargo tree -p concord_core --no-default-features -e normal,build,features` | Normal no-default production graph with mandatory Reqwest. |
| P0-WORK-005 | FEAT-DANGER-001 | feature-boundary | `concord_core/Cargo.toml` | `cargo check -p concord_core --lib --no-default-features --features dangerous-raw-response` | Selected independently. |
| P0-WORK-006 | FEAT-DANGER-001 | feature-boundary | `concord_core/Cargo.toml` | `cargo check -p concord_core --lib --no-default-features --features dangerous-custom-retry` | Selected independently. |
| P0-WORK-007 | FEAT-DANGER-001 | feature-boundary | `concord_core/Cargo.toml` | `cargo check -p concord_core --lib --no-default-features --features dangerous-dev-tools` | Selected independently. |
| P0-WORK-009 | CONF-CI-002 | release-command | `.config/nextest.toml` | `cargo nextest run --workspace` | Default retries=0; 156 tests. |
| P0-WORK-010 | COMPAT-DEP-002 | release-command | `deny.toml` | `cargo deny check` | Advisories, bans, licenses, sources; wildcards denied. |
| P0-WORK-011 | CONF-TRACE-005 | manual-review | `docs/conformance/CONFORMANCE_EVIDENCE_0.1.csv` | `manual comparison with docs/conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv` | Planned and actual catalogs remain separate. |
| P0-MACRO-001 | GEN-TYPE-001 | compile-pass | `concord_macros/tests/fixtures/normal/src/lib.rs` | `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/normal/Cargo.toml` | Normal downstream dependency at MSRV. |
| P0-MACRO-002 | COMPAT-MACRO-002 | compile-pass | `concord_macros/tests/fixtures/renamed_core/src/lib.rs` | `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/renamed_core/Cargo.toml` | Caller renames core. |
| P0-MACRO-003 | COMPAT-MACRO-003 | compile-pass | `concord_macros/tests/fixtures/nested_module/src/lib.rs` | `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/nested_module/Cargo.toml` | Invocation below crate root. |
| P0-MACRO-004 | COMPAT-MACRO-003 | compile-pass | `concord_macros/tests/fixtures/repeated_invocation/src/lib.rs` | `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/repeated_invocation/Cargo.toml` | Two isolated invocations. |
| P0-MACRO-005 | GEN-EXPL-002 | compile-pass | `concord_macros/tests/fixtures/equal_name/src/lib.rs` | `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/equal_name/Cargo.toml` | Covers only required outer-scope arguments, lexical ordering, and scope/endpoint separation; optional/defaulted scope controls are not implemented or proved. |
| P0-MACRO-006 | GEN-OWN-002 | compile-pass | `concord_macros/tests/fixtures/minimal_bounds/src/lib.rs` | `cargo +1.85.0 check --offline --locked --manifest-path concord_macros/tests/fixtures/minimal_bounds/Cargo.toml` | Non-Clone display-only fixture. |
| P0-MACRO-007 | COMPAT-GEN-001 | compile-fail | `concord_macros/tests/compile.rs` | `downstream_compile_fixtures_prove_resolution_hygiene_and_failures` | Contract-mismatch diagnostic. |
| P0-MACRO-011 | GEN-EXPL-002 | compile-fail | `concord_macros/tests/compile.rs` | `downstream_compile_fixtures_prove_resolution_hygiene_and_failures` | Missing-scope constructor typing; required scope slot only. |
| P0-MACRO-010 | GEN-EXPL-002 | downstream-generated | `concord_macros/tests/generated_surface.rs` | `explicit_constructor_preserves_distinct_scope_and_endpoint_values` | Covers only required outer-scope arguments, lexical ordering, and scope/endpoint separation; optional/defaulted scope controls are not implemented or proved. |
| P0-AUTH-001 | DSL-AUTH-003 | semantic-unit | `concord_core/src/auth/tests.rs` | `cargo test -p concord_core auth_model` | Private source-aware challenge-default model. |
| P0-AUTH-002 | DSL-AUTH-005 | semantic-unit | `concord_core/src/auth/tests.rs` | `auth_model_custom_provider_requires_explicit_policy` | Private policy model only. |
| P0-AUTH-003 | API-AUTH-003 | semantic-unit | `concord_core/src/auth/tests.rs` | `auth_model_rejects_two_challenge_enabled_placements` | Private plan model only. |
| P0-AUTH-004 | EXEC-AUTH-001 | concurrency | `concord_core/src/auth/tests.rs` | `auth_model_one_provider_operation_and_one_cohort_result` | Deterministic private single-flight model. |
| P0-AUTH-005 | EXEC-AUTH-002 | concurrency | `concord_core/src/auth/tests.rs` | `auth_model_one_provider_operation_and_one_cohort_result` | Deterministic private cohort model. |
| P0-AUTH-006 | EXEC-AUTH-005 | concurrency | `concord_core/src/auth/tests.rs` | `auth_model_manual_set_wins_over_stale_completion` | Private stale-completion model. |
| P0-AUTH-007 | EXEC-AUTH-008 | semantic-unit | `concord_core/src/auth/tests.rs` | `auth_model_static_rejection_is_persistent_without_provider_call` | Private rejection model. |
| P0-AUTH-008 | EXEC-RECOVER-008 | semantic-unit | `concord_core/src/auth/tests.rs` | `auth_model_only_source_replacement_clears_rejected_slot` | Private replacement model. |
| P0-AUTH-009 | EXEC-AUTH-010 | model-check | `concord_core/src/auth/systematic.rs` | `auth_model_systematic_finalization_boundary_clear_and_replacement_four_schedules` | Four exhaustive valid schedules. |
| P0-AUTH-010 | CONF-AUTH-002 | model-check | `concord_core/src/auth/systematic.rs` | `cargo test -p concord_core auth_model_systematic` | 36 candidates; 33 explored; 3 causal prunes. |
| P0-HTTP-001 | SEC-HTTP-003 | runtime | `concord_core/tests/reqwest_contract.rs` | `redirects_are_disabled` | Local Reqwest 0.13.4 probe. |
| P0-HTTP-002 | SEC-HTTP-004 | runtime | `concord_core/tests/reqwest_contract.rs` | `cookies_are_not_replayed` | Local Reqwest 0.13.4 probe. |
| P0-HTTP-003 | SEC-HTTP-005 | runtime | `concord_core/tests/reqwest_contract.rs` | `system_proxy_environment_is_ignored` | Isolated child-process probe. |
| P0-HTTP-004 | EXEC-COUNT-003 | runtime | `concord_core/tests/reqwest_contract.rs` | `retry_is_off_by_default` | One logical call, physical stream, and connection. |
| P0-HTTP-006 | EXEC-NATIVE-002 | runtime | `concord_core/tests/reqwest_contract.rs` | `hidden_retry_does_not_reenter_concord_boundaries` | Logical boundaries one; physical streams two. |
| P0-HTTP-007 | CONF-REQWEST-001 | runtime | `concord_core/tests/reqwest_contract.rs` | `cargo +1.85.0 test --offline --locked -p concord_core --test reqwest_contract -- --test-threads=1` | 13 deterministic local Reqwest 0.13.4 probes covering redirect/cookie/proxy defaults, retry counts and body replay, timeout distinction, local failure classification and connection observations. |
| P0-HTTP-008 | EXEC-STATUS-001 | runtime | `concord_core/tests/status_body_contract.rs` | `cargo +1.85.0 test --offline --locked -p concord_core --test status_body_contract` | 12 complete-only probes. |
| P0-HTTP-009 | EXEC-STATUS-004 | runtime | `concord_core/tests/status_body_contract.rs` | `unknown_oversized_body_returns_no_prefix` | Overflow discards prefix. |
| P0-HTTP-010 | EXEC-RECOVER-002 | runtime | `concord_core/tests/status_body_contract.rs` | `recoverable_challenge_body_is_unread` | Poll count zero. |
| P0-HTTP-011 | SEC-HTTP-008 | runtime | `concord_core/tests/https_auth_contract.rs` | `authenticated_http_fails_before_materialization` | Local application gate model. |
| P0-HTTP-012 | SEC-HTTP-009 | runtime | `concord_core/tests/https_auth_contract.rs` | `http_provider_request_fails_before_materialization` | Private provider-scheme proof; no public API. |
| P0-HTTP-013 | COMPAT-DEPS-001 | feature-boundary | `concord_core/Cargo.toml` | `cargo tree -p concord_core --no-default-features -e normal,build,features` | Reqwest-only direct runtime dependency. |
| P0-HTTP-014 | FEAT-WORKSPACE-001 | feature-boundary | `concord_core/Cargo.toml` | `cargo tree -p concord_core --no-default-features -e normal,build,dev,features` | Probe features/dependencies dev-only. |
| P0-PAGE-001 | EXEC-PAGE-009 | semantic-unit | `concord_core/src/pagination/tests.rs` | `cargo test -p concord_core pagination_identity` | Private identity model; canonical URL supplied. |
| P0-PAGE-002 | EXEC-PAGE-009 | semantic-unit | `concord_core/src/pagination/tests.rs` | `pagination_identity_hash_collision_uses_full_equality` | Private collision model. |
| P0-PAGE-003 | EXEC-PAGE-003 | semantic-unit | `concord_core/src/pagination/tests.rs` | `pagination_identity_rejects_immediate_repeat` | Private cycle detector. |
| P0-PAGE-004 | EXEC-PAGE-003 | semantic-unit | `concord_core/src/pagination/tests.rs` | `pagination_identity_rejects_longer_cycle` | Private nonadjacent cycle detector. |
| P0-PAGE-006 | BOUND-PAGE-008 | semantic-unit | `concord_core/src/pagination/tests.rs` | `pagination_item_limit_rejected_before_append` | Checked item ceiling model. |
| P0-PAGE-007 | BOUND-PAGE-009 | semantic-unit | `concord_core/src/pagination/tests.rs` | `pagination_identity_limit_rejected_before_retention` | Checked retained-byte ceiling model. |
| P0-PAGE-008 | EXEC-PAGE-006 | semantic-unit | `concord_core/src/pagination/tests.rs` | `pagination_returns_no_partial_result_on_failure` | Error without partial vector. |
| P0-PAGE-009 | API-PAGE-005 | semantic-unit | `concord_core/src/pagination/tests.rs` | `custom_controller_continues_after_short_page` | Private custom/built-in distinction. |
| P0-EXEC-001 | PROD-CORE-003 | semantic-unit | `concord_core/src/execute/tests.rs` | `executor_generated_and_prepared_use_one_shared_function` | Private typed executor. |
| P0-EXEC-003 | PROD-CORE-002 | semantic-unit | `concord_core/src/execute/tests.rs` | `executor_handler_output_is_compile_time_typed` | Private generic output; no runtime Any. |
| P0-EXEC-004 | API-PREP-001 | semantic-unit | `concord_core/src/execute/tests.rs` | `executor_generated_and_prepared_return_same_typed_success` | Private typed success parity. |
| P0-EXEC-005 | API-PREP-001 | semantic-unit | `concord_core/src/execute/tests.rs` | `executor_generated_and_prepared_return_same_status_error` | Scripted status parity. |
| P0-EXEC-006 | EXEC-AUTH-012 | semantic-unit | `concord_core/src/execute/tests.rs` | `executor_final_auth_failure_after_limiter_prevents_transport` | Reservation retained; no transport. |
| P0-EXEC-007 | EXEC-DEADLINE-001 | semantic-unit | `concord_core/src/execute/tests.rs` | `cargo test -p concord_core executor_deadline` | Four private logical-clock tests. |
| P0-EXEC-008 | EXEC-OPID-001 | semantic-unit | `concord_core/src/execute/tests.rs` | `executor_all_events_share_operation_id` | Single-operation correlation only. |

Known evidence gaps include every checker-related requirement; `CONF-CI-001` Rustdoc-warning failure enforcement; the optional/defaulted scope-control portion of `GEN-EXPL-002`; `GEN-EXPL-001` runtime parity; `GEN-EXPL-003` client-side no-inferred-scope behavior; `API-HTTP-002` safe retry opt-in API; `API-PHTTP-004` public `ProviderHttp` API; `BOUND-PAGE-007` and `EXEC-PAGE-010` requested page-limit preflight; complete `EXEC-COMPLETE-005`/`API-HOOK-007`; concurrent `EXEC-OPID-002`/`API-HOOK-008`; native production status classification; production generated/prepared execution; and managed Reqwest end-to-end execution. `cargo doc --workspace --no-deps` passed without warnings, but the maintained release command does not set `RUSTDOCFLAGS="-D warnings"`; therefore Rustdoc-warning failure enforcement is not proved. The lower-level `SEC-HTTP-009` provider-scheme/materialization invariant has private proof evidence. No passing or waived row claims the listed gaps.

## 8. Public Surface Audit

`concord_macros` exports the `api` proc macro. The generated subset emits a concrete client, generated support module, facade type, endpoint module/type, constructors, and endpoint methods with private fields. The only core production integration surface is doc-hidden `concord_core::__private`, containing the generated contract version/assertion and re-exported doc-hidden `RequestHead`; its fields remain private and it is inert.

The required greps found no `pub trait ... Transport`, no `pub trait ... Executor`, no public Reqwest request/response, and no capacity-hint names. The `std::any|TypeId|Box<dyn Any>|downcast` grep found only the forbidden-token array in `concord_core/src/execute/tests.rs`. Auth, pagination, and executor proof code is private and test-only. Public helpers in `concord_test_support` are acceptable because that crate is unpublished and exists only for tests. `cargo doc --workspace --no-deps` emitted no warning or unexpected production item.

## 9. Security Audit

- Credential proof models use numeric identities and never store actual secrets.
- Application and private provider-scheme gates reject before credential or provider-body materialization; localhost has no application exception. The provider proof does not implement the public `ProviderHttp` API.
- Request/response probe diagnostics avoid body bytes; terminal-body wrappers expose only finite complete captures and structural metadata.
- Recoverable challenge bodies remain unread; terminal challenges use the bounded complete-only path.
- Hidden native retry can create multiple physical streams while Concord logical hook, limiter, body-factory, finalization, and Reqwest-call counters remain one in the probe. Enabled native retry physical counts remain explicitly non-guaranteed.
- Proof models do not establish production credential isolation, redaction types, or managed transport; those are deferred.

## 10. Boundedness Audit

Status-body tests cover known excess without polling, unknown excess with complete prefix discard, read failure, deadline, small complete capture, terminal challenge, and recoverable unread behavior. Credential-model version, cohort, retry-time, and boundary counters use checked arithmetic and fail closed. Pagination tests cover checked page/item/identity counts, retained canonical byte totals, no-partial-result behavior, and one retained identity per accepted distinct page. Executor operation-ID allocation uses checked increment and fails before boundaries on overflow, but cross-runtime concurrent uniqueness is not proved.

The systematic auth scheduler enumerates finite event sets: 36 candidates, 33 valid explored, and 3 explicitly pruned. Local servers use finite scripts, three-second probe waits, finite bodies, explicit thread serialization for Reqwest probes, and bounded TLS/proxy helpers.

## 11. Architecture Boundary Audit

There is no public executor or transport abstraction, runtime `Any`, separate generated/prepared execution loop, public pagination runtime, public credential slot/state, or public native Reqwest request/response. Generated and prepared values enter the same private generic executor in the PR 0.6 proof. The production crate currently exposes only the macro-facing doc-hidden ABI; all runtime proof architectures remain test-only.

## 12. Confirmed and Disproved Assumptions

- Confirmed: Reqwest 0.13.4 default policy retried one HTTP/2 `REFUSED_STREAM` internally, producing two physical streams on one connection in this run.
- Confirmed: explicit `retry(never())` produced one physical stream for one logical call.
- Confirmed: hidden native retry did not re-enter the modeled Concord hook, limiter, body factory, final-auth, or execute-call boundaries.
- Confirmed as observation only: a fully consumed small HTTP/1.1 body allowed connection reuse; dropped or abandoned bodies led to a different connection in these probes.
- Confirmed: complete small terminal bodies are captured; known/unknown excess, read failure, and deadline expose no partial prefix while preserving status.
- Confirmed: caller-visible dependency renaming, nested invocation, and repeated invocation compile offline at MSRV.
- Confirmed in the reduced model: clear/replacement before finalization blocks stale material; after finalization request-local material cannot be retracted.
- Disproved: logical attempt count cannot be inferred from physical streams when native Reqwest retry is enabled.
- Disproved: connection reuse or closure after response abandonment is not a stable guarantee suitable for a normative claim.

## 13. Known Limitations

- The specification checker and negative checker tests are explicitly excluded; no checker evidence or waiver exists.
- Normative documents remain `Normative Implementation Candidate`, not release-final.
- Auth, pagination, and executor proofs are private models, not production implementations.
- Completion elapsed-time metadata is not implemented or proved; no `EXEC-COMPLETE-005` or `API-HOOK-007` evidence is claimed.
- Concurrent cross-runtime operation-ID uniqueness is not proved; no `EXEC-OPID-002` or `API-HOOK-008` evidence is claimed.
- Pagination proof assumes the logical URL has already been canonically produced.
- Reqwest HTTP/1.1 connection observations are not contractual guarantees.
- Native production status classification, production prepared endpoint execution, and managed Reqwest end-to-end execution remain future work.
- `CONF-CI-001` Rustdoc-warning failure enforcement remains unproved: `cargo doc --workspace --no-deps` passed without warnings, but the maintained release command does not set `RUSTDOCFLAGS="-D warnings"`.
- The `GEN-EXPL-002` evidence covers only required outer-scope arguments, lexical ordering, and scope/endpoint separation; optional/defaulted scope controls are not implemented or proved.
- `GEN-EXPL-001` runtime parity and `GEN-EXPL-003` client-side no-inferred-scope behavior remain unproved.
- `API-HTTP-002` safe retry opt-in and `API-PHTTP-004` public `ProviderHttp` APIs remain unimplemented and unproved; the raw retry observation and private `SEC-HTTP-009` scheme gate do not substitute for those APIs.
- `BOUND-PAGE-007` and `EXEC-PAGE-010` requested page-limit preflight remain unproved; the removed internal page-ceiling mapping did not establish that boundary.
- Cargo Deny reports accepted duplicate-version warnings for `core-foundation` and `windows-sys` while all deny checks pass.

## 14. Deferred Requirements

Phase 0 proves workspace, compile contracts, dependency assumptions, and bounded reduced models. Milestone 1 must begin the generated vertical implementation without treating private models as runtime code. Explicitly deferred are `CONF-CI-001` Rustdoc-warning failure enforcement; the optional/defaulted scope-control portion of `GEN-EXPL-002`; `GEN-EXPL-001` runtime parity; `GEN-EXPL-003` client-side no-inferred-scope behavior; `API-HTTP-002` safe retry opt-in; `API-PHTTP-004` public `ProviderHttp`; and requested page-limit preflight under `BOUND-PAGE-007` and `EXEC-PAGE-010`. Production client construction, prepared endpoints, managed Reqwest transport, public typed errors/results, auth storage/providers, pagination, rate limiting, hooks, completion metadata, operation-ID concurrency, and full evidence completeness remain deferred to their planned milestones. No Milestone 1 source work is part of PR 0.7.

## 15. Milestone 1 Recommendation

`READY_WITH_NON_BLOCKING_LIMITATIONS`

The source and integrated non-checker gates are sound enough to begin Milestone 1 after coordinator acceptance of PR 0.7. The checker exclusion remains an explicit Phase 0 governance limitation and must be reversed and satisfied before any future unqualified Phase 0 completion or release-final claim.
