# Testing

Concord uses several layers of tests because the project spans macro syntax, generated Rust, runtime behavior, and public docs.

## Macro Tests

Trybuild pass and fail fixtures cover public macro UI contracts: downstream compile boundaries, intended user-facing diagnostics, and span-sensitive diagnostics. Fixtures are split by category under `concord_macros/tests/trybuild/`.

The current Trybuild functions are grouped by test binary.

`trybuild_current` contains:

- `trybuild_facade_contract_fixtures`
- `trybuild_endpoint_io_contract_fixtures`
- `trybuild_pagination_contract_fixtures`
- `trybuild_auth_contract_fixtures`
- `trybuild_route_contract_fixtures`
- `trybuild_codegen_contract_failures`

`trybuild_sema` contains:

- `trybuild_parser_diagnostics`
- `trybuild_route_diagnostics`
- `trybuild_auth_diagnostics`
- `trybuild_policy_diagnostics`
- `trybuild_pagination_diagnostics`

`trybuild_codegen` contains:

- `trybuild_codegen_contract_diagnostics`
- `trybuild_rust_type_errors`

Run the full trybuild suite with:

```bash
cargo nextest run -p concord_macros --test trybuild_current
cargo nextest run -p concord_macros --test trybuild_sema
cargo nextest run -p concord_macros --test trybuild_codegen
```

Refresh trybuild stderr output only when macro diagnostics intentionally change:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current -- --test-threads=1
```

Category-specific refresh examples:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_facade_contract_fixtures -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_parser_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_auth_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_policy_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_pagination_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_route_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_codegen trybuild_codegen_contract_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_codegen trybuild_rust_type_errors -- --test-threads=1
```

Only use `TRYBUILD=overwrite` when diagnostics intentionally change. Inspect the git diff of `.stderr` files before accepting updates. Path-only changes from fixture moves are acceptable; changed wording and spans must be reviewed.

The repo ships `.config/nextest.toml`. It gives `concord_macros`'s `trybuild_current` binary a longer slow-timeout and places it in the `trybuild` nextest group. The other trybuild binaries use the ordinary nextest scheduling.

The current-pass example above is representative; the other current-pass
wrapper functions listed under `trybuild_current` use the same test binary.
`trybuild_codegen_contract_failures` is the compile-fail group in that binary.

Trybuild remains part of the full gate through `cargo nextest run --workspace --all-targets`. The checked-in nextest config only special-cases `concord_macros`'s `trybuild_current` binary for a longer timeout and the `trybuild` group; the other trybuild binaries run under the standard nextest scheduling.

Parser unit tests cover smaller syntax rules and span-sensitive diagnostics.

Sema unit tests cover name resolution, inheritance, policy merging, profile expansion, and diagnostics that need semantic context.

Codegen tests should prefer generated API compile checks, type checks, trybuild fixtures, and focused generated-shape assertions that cannot be expressed through Rust type checking.

Macro strictness belongs primarily in semantic unit tests and trybuild pass/fail fixtures. Add trybuild fail fixtures when a rejected form needs a stable public diagnostic. Source-level keyword audits can be useful during review, but they should not be normal `cargo test` checks.

`just release` validates both the all-feature and default-feature workspace
configurations. Each configuration is checked, linted with warnings denied,
and exercised through Nextest. The release gate also compiles `concord_core`
with Rust 1.97 at both feature extremes: no default features and all features.

Supply-chain policy is gated by `just supply-chain`. It requires `cargo-deny`, checks advisories, yanked crates, licenses, dependency sources and registries, and configured ban policy, and it may require a cached advisory database or network access to refresh advisory data. It does not use live credentials.

The executable workspace-test axes in the canonical release gate are:

```text
cargo nextest run --workspace --all-targets --all-features \
  --no-tests fail --no-fail-fast --retries 0
cargo nextest run --workspace \
  --no-tests fail --no-fail-fast --retries 0
```

Focused per-crate, UI, individual-feature, and no-default executable-test
filters remain optional diagnostics. The no-default Core compilation check is
not optional: `just release` runs
`cargo +1.97 check -p concord_core --no-default-features`.

The no-default rate-limit regression is exercised separately with a focused cargo test filter instead of the full runtime suite:

```bash
cargo test -p concord_core --no-default-features no_default_rate_limit
cargo test -p concord_core --no-default-features --features json no_default_rate_limit
```

## Architecture Boundary Checks

Architecture boundaries are maintained through module and crate organization,
compile-fail/runtime tests, review, and focused repository searches for public
execution and request-extension boundaries.

The maintained architectural contract includes:

- `concord_core` must not depend on `concord_macros`.
- `concord_core` must not reference DSL, parser, or raw AST concepts.
- codegen must consume resolved semantic data instead of raw syntax trees.
- codegen must not rely on validation-dependent panics for semantic invalid states.
- codegen review should avoid validation-dependent panics and direct `.unwrap()` in semantic rendering.

When a targeted test or review identifies a boundary regression, fix the layer
organization instead of weakening the contract.

## Core Tests

`concord_core` has runtime characterization tests for concurrency, rate-limit, auth rejection, retry, decode, pagination, codecs, and runtime configuration.

These tests protect runtime order and should be extended before runtime behavior is refactored.

Auth and redaction tests cover arbitrary auth names and verify that response values, debug sinks, and errors do not contain raw auth material while the native request carries credentials only at execution.

Auth preparation boundary tests verify that raw material stays out of logical, debug, and error surfaces and reaches only the native request at execution time.

Runtime strictness tests should reject invented policy values and silent saturation through observable behavior. Rate-limit `[host]` keys must fail explicitly when the logical URL has no host. Request and authentication counters should return typed overflow errors instead of saturating.

Runtime lock and state tests should poison representative auth and rate-limit state where feasible and assert typed errors instead of panics.

Response body limit tests should cover `Content-Length` precheck, unknown-length and chunked enforcement, exactly-at-limit success, decode bypass on oversized bodies, auth HTTP token response limits, and separation between endpoint response read limits and auth-internal response limits.

## Deterministic Async Harness

The deterministic native executor is available only with
`dangerous-dev-tools`. Configure a `SafeReqwestBuilder` with a handle created
for either application or provider execution through
`concord_core::__development`; channel mismatch is rejected. The managed
application and provider handles are consumed during client construction.
Scripts provide native status, headers, buffered or chunked bodies,
trailers, body gates, partial body failure, and focused synthetic execution
failure categories. Successful scripts always enter Core as
`reqwest::Response` values and therefore exercise the normal response pipeline.

Default captures expose logical pre-auth request metadata and body shape only.
Use `UnsafeCredentialPlacementExpectations` solely with deterministic fake
credentials when a test must prove native header/query placement. It compares
values inside the executor and returns only a redacted request-category failure
on mismatch; no raw native request accessor exists.

The executor consumes native request bodies through the same `reqwest::Body`
adaptation used by production execution. `UnsafeRequestBodyExpectations` may
compare explicitly fake bytes inside the executor, but values are never
returned or formatted. Ordinary captures expose only body category and known
length. Request-head observations and the mutually exclusive body terminal
states (`completed`, `failed`, `cancelled/dropped`, and `never polled`) cover
producer polling, limits, exact-length errors, gating, and cancellation
without a second request pipeline. Successful completion is emitted only at
terminal EOF, and each execution emits at most one body terminal state.

Synthetic timeout scripts prove configured timeout propagation through
`CapturedNativeRequest::timeout()` and stable Concord error mapping. They do
not introduce a wall-clock delay or claim to test Reqwest's deadline
enforcement; actual deadline behavior remains upstream Reqwest behavior.

Focused foundation diagnostics are:

```bash
cargo test -p concord_core --all-features development
cargo test -p concord_core --all-features executor
cargo test -p concord_core --all-features capture
cargo test -p concord_core --all-features provider
```

TLS preflight coverage includes application and provider ordering, HTTP in a
no-TLS build, TLS-enabled synthetic HTTPS, dynamic-origin and later-page
changes, body-factory/non-polling assertions, error redaction, and untouched
executor scripts. The no-default feature-boundary fixture is run with
`dangerous-dev-tools` only to provide offline deterministic native execution;
that feature does not alter or override the managed TLS capability.

`tests/development_boundary.rs` performs an external offline compile check: the
fixture fails without `dangerous-dev-tools` and compiles when the feature is
enabled. Repository boundary checks also keep executor symbols out of `prelude`,
`advanced`, generated integration, and ordinary client constructors.

Maintained Core, macro-generated, public-extension, and test-support tests use
`concord_test_support::DeterministicMock`, `ScriptedReply`,
`MockExecutionHandle`, and `ResponseGate`. They open no listener, rewrite no
URL, and install no proxy. Generated clients remain concrete; their ordinary
safe-builder constructor is configured by the feature-gated harness, with no
generated development trait, selector, field, or executor generic.

Examples and performance fixtures use the same feature-gated deterministic
native executor. No maintained test or benchmark opens a local listener,
rewrites request origins, or counts hidden Reqwest protocol sends.

The dangerous development feature does not persist request or response bodies.

Every harness wait is bounded through `wait_bounded`, `PhaseGate::wait_for`, or `PhaseGate::try_wait_for`. Tests that assert a task is still blocked may use a short bounded negative wait such as `assert_still_pending`, but the phase event must be the synchronization point. Do not make correctness depend on arbitrary wall-clock sleeps.

Harness observations must remain safe metadata. They may include sanitized URLs,
statuses, and headers, but not raw auth material or secret values.

## Examples And Docs Tests

`concord_examples` compile-checks public usage. It includes small examples, public docs fixtures, generated API usage tests, and the Riot fixture.

The Riot fixture is the large real-world surface test. Do not change Riot semantics for unrelated macro or runtime work.

Markdown prose is not validated by keyword tests in `cargo test`. Release review may include a manual source and documentation audit for outdated wording, unsafe live calls, or validation-dependent codegen patterns, but that audit is not a substitute for parser, compile, diagnostic, and runtime tests.

## Full Local Gate

Run the canonical maintained-workspace gate with:

```bash
just release
```

It runs formatting verification; all-target checks, strict Clippy, and Nextest
for both all-feature and default-feature workspace configurations; Rust 1.97
Core compilation checks with no default features and with all features;
doctests; rustdoc; and the supply-chain policy check. It also runs
`perf-check`, `perf-test`, and `bench-check`, so performance compilation,
deterministic performance tests, and benchmark compilation are part of the
release gate.

## New DSL Feature Checklist

1. Add or update parser AST.
2. Add parser success and fail fixtures.
3. Add semantic model fields.
4. Add sema resolution and diagnostics.
5. Add a merge and inheritance test if applicable.
6. Add codegen.
7. Add core runtime support only if required.
8. Add public docs.
9. Add compiled example if public syntax.
10. Add dev docs if architecture changes.
11. Run full verification.
