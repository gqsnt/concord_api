# Phase 1 Integrated Conformance Report

## Verdict

`PHASE_1_COMPLETE_WITH_LIMITATIONS`

All 18 Phase 1 exit criteria in the implementation mission are satisfied. The limitations below do not invalidate those criteria: they identify intentional Phase 1 boundaries, reduced evidence, and deferred APIs whose final dependent public types do not yet exist.

This is a Phase 1 result, not a claim of full Concord production conformance.

## Workspace Identity

- Repository: `F:\projects\concord_api`
- Branch: `master`; no branch was created or switched.
- Phase 0 final commit: `d21de926a1ff4215279d74ab768e58660333a1d1`
- Accepted Phase 0 evidence anchor: `3dbea65ab7e94a8b779329b895b314223161cdf4`
- PR 1.1 baseline: `9c335396fb7bb0081c1239f33937161ea59d946d`
- Integrated Phase 1 source commit tested by PR 1.6: `1074c141fd1a48748d08a46bfca66fb6201e0b61`
- PR 1.7 correction baseline: `4c1bb03587a555f03cc5aa1866656c15069e94c3`
- PR 1.7 corrected candidate: the tree recorded by the containing PR 1.7 commit
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Managed HTTP baseline: Reqwest 0.13.4 with Reqwest default features disabled
- Validation date: 2026-07-17 UTC

The source tree was clean at the start and end of validation against `1074c141fd1a48748d08a46bfca66fb6201e0b61`. PR 1.6 changed only this report and the actual-evidence catalog. PR 1.7 began from clean commit `4c1bb03587a555f03cc5aa1866656c15069e94c3`, corrected the integrated governance and source defects listed below, and reran the expanded release matrix before commit.

## Accepted Phase 1 Units and Commits

| Unit | Commit | Subject |
|---|---|---|
| PR 1.1 | `9c335396fb7bb0081c1239f33937161ea59d946d` | `phase 1.1` |
| PR 1.2 | `3932c8fcd0717a75ba95462f384043a157febaa1` | `feat(core): add structured safe errors` |
| PR 1.3 | `ff998924ce0505056fbb8f95e020c549db00dbb4` | `feat(core): add safe http and runtime configuration` |
| PR 1.4 | `54097a6e1c7fd41d3fb7380ccf0005b67ba0d86a` | `feat(core): add managed reqwest clients` |
| PR 1.5 | `1074c141fd1a48748d08a46bfca66fb6201e0b61` | `test(core): add deterministic reqwest boundary` |
| PR 1.6 | containing commit | `phase1: record integrated validation and evidence` |
| PR 1.7 | containing commit | `phase1: correct integrated governance and evidence` |

Each unit has an ignored committed patch under `git_diff/phase1-pr-1.X.patch`. The patch directory is not tracked.

## Production Surface Added

The stable public Phase 1 surface is intentionally small:

- central sanitized metadata: `SanitizedHeaders`, `SanitizedHeaderMap`, and `SanitizedHeaderValue`;
- structured errors: `ApiError`, `ErrorCategory`, `ClientBuildError`, `ConfigurationError`, their public classification enums, `HttpResponseFailure`, `StatusBody`, `StatusBodyState`, and `StatusBodyDecodeError`;
- explicit pagination ceiling classifications: `PageCeilingExceeded`, `CollectedItemCeilingExceeded`, and `IdentityStorageCeilingExceeded`;
- final terminal-body decoding contract: `CodecError`, `DecodeContext`, and `DecodeBody`;
- safe configuration: `HttpConfigBuilder`, `ProviderRetry`, `RuntimeConfigBuilder`, and `SafeProxy`;
- exact feature-gated native retry exposure in `dangerous` under `dangerous-custom-retry`;
- unstable deterministic harness types only in `__development` under `dangerous-dev-tools`.

Private-only foundations are:

- immutable resolved HTTP/runtime configuration;
- `ApplicationHttp` and `ProviderHttpInner`, each owning a separate concrete Reqwest client and pool;
- TLS capability and safe client-build reduction;
- the complete-only terminal status-body collector;
- the final-call deterministic seam inside the two managed wrappers.

No `ClientCore`, generated client construction, production endpoint execution, response-success pipeline, auth provider, limiter, pagination runtime, hook execution, stream, multipart, or built-in codec family was added.

## Retained Phase 0 Proof Models

The private auth, pagination, and typed-executor models remain under `cfg(test)` in `concord_core/src/auth`, `concord_core/src/pagination`, and `concord_core/src/execute`. They remain reduced proofs and are not production runtime implementation.

The local Reqwest, status-body, and HTTPS/auth contract probes were retained where they still test dependency behavior or invariants not fully integrated into a production endpoint path. Evidence rows anchored to the Phase 0 source commit were not rewritten. Phase 1 evidence distinguishes production values and private production helpers from retained probes.

## Integrated Commands

The following were executed successfully against source commit `1074c141fd1a48748d08a46bfca66fb6201e0b61`:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | passed |
| `cargo +1.85.0 check --workspace --all-targets` | passed on Rust 1.85.0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | passed |
| `cargo test --workspace` | passed; 230 tests total, no failures |
| `cargo nextest run --workspace` | passed; 230/230 across 23 binaries, 0 skipped |
| `cargo check --workspace --no-default-features` | passed |
| `cargo check --workspace --all-features` | passed |
| `cargo deny check` | passed advisories, bans, licenses, and sources; emitted non-failing duplicate-version warnings for `core-foundation` and `windows-sys` |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | passed |
| `git diff --check` | passed |
| `just release` | passed under the then-current PR 1.6 recipe; PR 1.7 supersedes that release matrix below |

Focused validation was also rerun explicitly:

- PR 1.1: `cargo test -p concord_core redaction` (10 passed), `--test sanitized_metadata` (1 passed), and `--test raw_escape_compile_fail` (1 passed).
- PR 1.2: `cargo test -p concord_core error` (16 passed), `--test error_api` (2 passed), `--test error_redaction` (1 passed), and `--test status_body_api` (2 passed).
- PR 1.3: `--test http_config` (1 passed), `--test runtime_config` (1 passed), `--test proxy` (2 passed), and `--test config_methods` (1 passed, including downstream feature fixtures).
- PR 1.4: private managed-client tests (12 passed serially), private status collector tests (5 passed), all-feature collector tests (6 passed including post-gzip-decompression accounting), no-default HTTP/TLS preflight (1 passed), and the `managed_clients`, `https_auth_contract`, `status_body_contract`, and `reqwest_contract` targets (1, 6, 12, and 13 passed respectively).
- PR 1.5: private seam tests (3 passed serially), `development` (3 passed with downstream presence/absence/signature fixtures), `deterministic_boundary` (3 passed), `deterministic_redaction` (1 passed), and `concord_test_support` (2 passed).
- `cargo tree -p concord_core --no-default-features -e normal,build,features` confirmed Reqwest 0.13.4 and zero normal `concord_test_support` references.

No removed specification automation or recreated equivalent was invoked.

### PR 1.7 correction validation

The corrected candidate tree passed the following expanded matrix:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | passed |
| `cargo +1.85.0 check --workspace --all-targets --all-features` | passed on Rust 1.85.0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `cargo test --workspace` | passed; 234 tests total, no failures |
| `cargo nextest run --workspace --all-features` | passed; 237/237 across 25 binaries, 0 skipped |
| `cargo test --workspace --doc --all-features` | passed; three doc-test targets, 0 tests, no failures |
| `cargo check --workspace --no-default-features` | passed |
| `cargo check --workspace --all-features` | passed |
| `cargo +1.85.0 check -p concord_core --no-default-features` | passed |
| `cargo +1.85.0 check -p concord_core --all-features` | passed |
| `cargo +1.85.0 check -p concord_macros` | passed |
| `cargo deny check` | passed advisories, bans, licenses, and sources; retained the two non-failing duplicate-version warnings below |
| `just release` | passed the corrected 14-command recipe; all-feature Nextest was 237/237, no-default target was 1/1, and the auth-model filter was 51/51 |
| `git diff --check` | passed |

Focused correction tests also passed:

- `cargo test -p concord_core --test compression_features -- --test-threads=1`: 1 passed; each compression feature produced only its matching Reqwest feature edge.
- `cargo test -p concord_core --test error_api`: 3 passed; the downstream public API references all eight pagination classifications.
- `cargo test -p concord_core pagination_ceiling_kinds`: 2 relevant tests passed across the unit/public targets; the remaining tests were filtered.
- `cargo test -p concord_core --no-default-features --test no_default`: 1 passed and executed the existing private managed HTTP/TLS no-default test.

The maintained `just release` recipe now contains the applicable normative all-feature workspace MSRV check, all-feature Clippy, all-feature Nextest, all-feature doc tests, PowerShell-scoped `RUSTDOCFLAGS=-D warnings`, no-default core check/test/MSRV gates, all-feature core MSRV, macro MSRV, the auth-model filter, Cargo Deny, and Git whitespace validation. It contains no specification automation command.

## Requirement Evidence Added

PR 1.6 appended 63 Phase 1 evidence rows to `CONFORMANCE_EVIDENCE_0.1.csv`:

- 3 integrated workspace rows;
- 6 central-redaction rows;
- 16 error/status-body rows;
- 16 configuration/default-limit rows;
- 16 managed-HTTP rows;
- 6 deterministic-boundary rows.

All 63 PR 1.6 rows reference source commit `1074c141fd1a48748d08a46bfca66fb6201e0b61` and exact commands or tests. PR 1.7 adds one direct feature-tree row anchored to exact committed and tested source `4c1bb03587a555f03cc5aa1866656c15069e94c3`. The catalog therefore contains 64 Phase 1 rows: 50 direct rows and 14 rows beginning with an explicit `partial` qualification.

PR 1.7 corrects one earlier mapping without changing its source commit or execution facts: `P1-HTTP-016` now maps the gzip collector test to `BOUND-BODY-011`, the explicit post-decompression response-accounting requirement. `FEAT-COMPRESS-001` is mapped only to `P1-CORR-001`, whose three Cargo feature-tree commands directly prove gzip, Brotli, and Deflate isolation. No row converts retained private auth/pagination/executor models into a production claim.

## Public Surface Audit

- `prelude` exports only `Bytes` and the common structured error types.
- `advanced` contains the required error, decoder, safe metadata, HTTP, and runtime configuration values.
- `dangerous` exists only with `dangerous-custom-retry` and exports only Reqwest's exact retry builder type.
- `__development` exists only with `dangerous-dev-tools`; compile fixtures prove it is absent otherwise and absent from `prelude`, `advanced`, and `dangerous`.
- Existing `__private` remains the generated-code contract boundary from Phase 0.
- Compile fixtures prove managed Reqwest owners/builders/clients and raw sanitized headers do not escape.
- Generated client types have no executor generic and no executor constructor argument with development tools enabled.

The audit found no public `Transport`, production `Executor`, request/response/auth plan, endpoint executor, runtime `Any`, generated-client transport generic, raw Reqwest client/builder accessor, credential version/lease, or pagination runtime adapter.

## Error and Redaction Audit

- Error categories are structural and matchable without parsing text.
- `PaginationErrorKind` now distinguishes hard caps, configured page ceilings, collected-item ceilings, and identity-storage ceilings without implementing pagination runtime behavior.
- Raw Reqwest errors are immediately reduced; no raw native error is retained as a source.
- Sentinel tests cover authenticated native URLs, request-body errors, terminal bodies, sensitive headers, codec error text, fake expectations, and deterministic mismatch diagnostics.
- `StatusBody` is finite and complete-only. Bytes exist only for `Complete`; every unavailable/omitted state returns no bytes.
- `StatusBody` has no Concord-provided `Clone`, `Deref<[u8]>`, `AsRef<[u8]>`, or serialization surface.
- Error and response formatting is structural and body-free; recursive source checks are sentinel-free.
- Retry-After parsing covers non-negative delta seconds, HTTP dates, past-as-zero, malformed-as-absent, and cap clamping.
- The production collector omits known-too-large bodies before polling, discards unknown-overflow prefixes, and preserves status on read/deadline failure.

## Managed HTTP Security Audit

- Application and provider traffic use separate private Reqwest clients and connection pools.
- Both builders disable redirects and implicit system proxy discovery. Reqwest's cookie feature is not enabled, and local tests prove cookies are not replayed.
- Only an explicit validated credential-free HTTP(S) origin can become a proxy.
- Application retry starts disabled. Reqwest-default retry requires explicit opt-in; physical native attempts remain inside one Concord boundary call.
- Provider retry is limited to its reviewed modes and has no dangerous application retry selection.
- Provider request preparation is HTTPS-only. Protected application HTTP and provider HTTP are rejected before later credential/body work, with no localhost exception.
- No-default unauthenticated HTTP succeeds. HTTPS produces typed TLS-unavailable preflight behavior and is never downgraded.
- Root certificates and identity PEM are parsed during construction under `default-tls`; invalid inputs are reduced without reproducing PEM bytes.
- No certificate-verification bypass, raw Reqwest escape, direct production `h2` dependency, or system-proxy opt-in was added.

## Feature and MSRV Audit

- Reqwest remains pinned to 0.13.4 with default features disabled.
- `default-tls`, `http2`, `gzip`, `brotli`, and `deflate` are wired explicitly.
- Feature compile fixtures prove presence and absence of HTTP/2, TLS PEM, decompression-disable, dangerous retry, and development methods/modules.
- Direct Cargo feature-tree evidence proves `gzip`, `brotli`, and `deflate` are independently selectable and each enables only its matching Reqwest feature.
- All-feature post-decompression byte accounting was executed for gzip and is mapped only to `BOUND-BODY-011`. Brotli and Deflate were compile-validated but not given separate runtime decompression probes.
- The no-default production graph retains Reqwest and ordinary HTTP but excludes the optional deterministic support dependency.
- Rust 1.85.0 workspace/all-target validation passed.
- `.gitattributes` establishes LF for Rust, TOML, Markdown, CSV, YAML, Cargo lockfiles, and the Justfile. Renormalization changed no tracked file solely for line endings; every changed tracked file has an intentional correction described in this report.

Specification governance is now explicitly manual and recorded: normative documents are reviewed in ownership/dependency order, the coverage plan is reviewed separately from actual evidence, conformance evidence is reviewed for exact executed artifacts and source commits, and the maintained release command is audited. Stale automation paths and command authority were removed from the normative, planning, contributor, review-resolution, documentation-map, and coverage-plan materials. No replacement checker was added.

## Deterministic Boundary Audit

- The seam is inside each private managed wrapper at the final native execution call.
- Application and provider channels have independent FIFO scripts for responses, terminal statuses, safe failures, gates, and cancellation points.
- Gates deterministically pause before native execution and at application/provider response delivery.
- Cancellation requeues a reserved script and does not consume the other channel.
- Ordinary captures contain only channel, endpoint, method, supplied safe logical URL, centrally sanitized header structure, body category, and known length.
- Captures contain no request-body bytes, credentials, native authenticated URL, or proxy value.
- Fake body/URL expectations remain internal; their `Debug`, mismatch errors, and mapped `ApiError` output are value-free.
- The optional dependency and all development fields/branches are removed by cfg when `dangerous-dev-tools` and `cfg(test)` are absent.

## Known Limitations and Deferred Requirements

1. Phase 1 intentionally has no production generated endpoint pipeline. Managed preflights, terminal collection, and the deterministic seam therefore have private production-helper evidence, not end-to-end generated request evidence.
2. `RuntimeConfigBuilder::hooks`, `rate_limiter`, and `debug` are deferred because `RuntimeHooks`, `RateLimiter`, and `DebugLevel` are later public types. No placeholder type or altered eventual signature was introduced.
3. Authentication providers, rate limiting, pagination runtime, hooks, bodies/responses, streams, multipart, successful codecs/handlers, prepared requests, and full deadline/recovery behavior remain future milestones. Their Phase 0 models are explicitly reduced evidence only.
4. Runtime post-decompression accounting was executed with gzip. Brotli and Deflate feature isolation is directly proved, but separate Brotli/Deflate runtime probes are not claimed.
5. `cargo deny check` passed while reporting duplicate transitive versions of `core-foundation` and `windows-sys`; these are warnings, not Phase 1 validation failures.
6. PR 1.7 does not resolve or claim resolution of the separate cleartext error-category or TLS-material design decision. TLS configuration was not redesigned.

These limitations do not invalidate any Phase 1 exit criterion.

## Milestone 2 Readiness

The Phase 1 source is internally consistent and ready for a separately authorized next milestone. Normative documents remain implementation candidates rather than being edited to match implementation convenience.

`NOT_STARTED`
