# Testing

Concord uses several layers of tests because the project spans macro syntax, generated Rust, runtime behavior, and public docs.

## Macro Tests

Trybuild pass and fail fixtures cover public macro UI contracts: downstream compile boundaries, intended user-facing diagnostics, and span-sensitive diagnostics. Fixtures are split by category under `concord_macros/tests/trybuild/`.

The current trybuild test functions are:

- `trybuild_facade_contract_fixtures`
- `trybuild_endpoint_io_contract_fixtures`
- `trybuild_pagination_contract_fixtures`
- `trybuild_auth_contract_fixtures`
- `trybuild_retry_contract_fixtures`
- `trybuild_route_contract_fixtures`
- `trybuild_parser_diagnostics`
- `trybuild_auth_diagnostics`
- `trybuild_policy_diagnostics`
- `trybuild_pagination_diagnostics`
- `trybuild_route_diagnostics`
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

The current-pass example above is representative; the other current-pass wrapper names (`trybuild_endpoint_io_contract_fixtures`, `trybuild_pagination_contract_fixtures`, `trybuild_auth_contract_fixtures`, `trybuild_retry_contract_fixtures`, and `trybuild_route_contract_fixtures`) use the same `--test trybuild_current` binary.

Trybuild remains part of the full gate through `cargo nextest run --workspace --all-targets`. The checked-in nextest config only special-cases `concord_macros`'s `trybuild_current` binary for a longer timeout and the `trybuild` group; the other trybuild binaries run under the standard nextest scheduling.

Parser unit tests cover smaller syntax rules and span-sensitive diagnostics.

Sema unit tests cover name resolution, inheritance, policy merging, behavior expansion, and diagnostics that need semantic context.

Codegen tests should prefer generated API compile checks, type checks, trybuild fixtures, and focused generated-shape assertions that cannot be expressed through Rust type checking.

Macro strictness belongs primarily in semantic unit tests and trybuild pass/fail fixtures. Add trybuild fail fixtures when a rejected form needs a stable public diagnostic. Source-level keyword audits can be useful during review, but they should not be normal `cargo test` checks.

Feature-surface drift is gated separately by `scripts/check_features.sh`. That script uses normal dependency trees for the crate-surface proof so dev-dependencies do not widen the default feature story. `scripts/check_v1.sh` calls it before the rest of the local gate.

The runtime nextest matrix is separate from the compile/check feature matrix. The checked-in local gate currently runs `cargo nextest run -p concord_core`, `cargo nextest run -p concord_core --all-features`, `cargo nextest run -p concord_examples`, `cargo nextest run -p concord_examples --all-features`, `cargo nextest run --workspace`, `cargo nextest run --workspace --all-features`, and `cargo nextest run --workspace --all-targets`. Feature-flavored core nextest runs such as `cargo nextest run -p concord_core --no-default-features` and `cargo nextest run -p concord_core --no-default-features --features json` are intentionally omitted for now because the core runtime suite is not feature-parametric and those runs fail in rate-limit characterization tests.

The no-default rate-limit regression is exercised separately with a focused cargo test filter instead of the full runtime suite:

```bash
cargo test -p concord_core --no-default-features no_default_rate_limit
cargo test -p concord_core --no-default-features --features json no_default_rate_limit
```

## Architecture Boundary Checks

Run:

```bash
bash ./scripts/check_architecture.sh
```

This protects the crate and compiler/runtime split:

- `concord_core` must not depend on `concord_macros`.
- `concord_core` must not reference DSL, parser, or raw AST concepts.
- codegen must consume resolved semantic data instead of raw syntax trees.
- codegen must not rely on validation-dependent panics for semantic invalid states.
- direct `.unwrap()` is intentionally banned in codegen by this gate so panic-prone semantic rendering does not creep back in under a different shape.

If the check fails, fix the layer boundary instead of weakening the pattern. Only add a narrow allowlist if a future PR has documented a real exception.

## Core Tests

`concord_core` has runtime characterization tests for concurrency, rate-limit, auth rejection, retry, decode, pagination, codecs, and runtime configuration.

These tests protect runtime order and should be extended before runtime behavior is refactored.

Auth and redaction tests must cover arbitrary auth names, not only conventional names such as `Authorization` or `api_key`. Basic auth usernames declared as `secret` are secret material too. When auth handling changes, verify that `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, debug sinks, and errors do not contain raw auth material, while the materialized `TransportRequest` still carries real credentials at `Transport::send`.

Auth preparation boundary tests should verify behavior at the sealed auth boundary: raw material stays out of logical request, debug, and error surfaces and reaches only `TransportRequest` at send time.

Runtime strictness tests should reject invented policy values and silent saturation through observable behavior. Rate-limit `[host]` keys must fail explicitly when the logical URL has no host. Request and auth attempt counters should return typed overflow errors instead of saturating.

Runtime lock and state tests should poison representative auth and rate-limit state where feasible and assert typed errors instead of panics.

Response body limit tests should cover `Content-Length` precheck, unknown-length and chunked enforcement, exactly-at-limit success, decode bypass on oversized bodies, auth HTTP token response limits, and separation between endpoint response read limits and auth-internal response limits.

## Deterministic Async Harness

Runtime async, cancellation, and drop tests should use the test-only harness in `concord_core/tests/integration/current_core/common.rs` instead of sleeps or stress loops.

The common helpers are:

- `PhaseGate`: records phase entry, waits for a phase count with a bounded timeout, blocks entrants, releases one waiter, releases all waiters, and preserves an ordered phase log.
- `PhaseGate`: release accounting is exact. Duplicate releases do not create surplus permits for future entrants, and cancelled blocked waiters clean up their own accounting without leaking a future release.
- `DropProbe`: creates cloneable drop tokens, counts drops, and waits for a drop count with a bounded timeout. It also tries to log a labeled drop event when drop-time locking is available, but the count is the authoritative signal.
- `GateableTransport`: records transport send start and request metadata, blocks at `transport_send`, returns configured responses or transport errors, counts sends, and can attach a `DropProbe` to the in-flight send future.
- `GateableBodyTransport`: returns a streaming response body that blocks at `body_chunk`, counts chunks read, can produce deterministic partial reads, and can attach a `DropProbe` to the body stream.
- `CountingRateLimiter`: records acquire start and completion, permit creation, response observation, and deterministic lifecycle completion. The public runtime permit type is currently a unit value, so this helper records the observable lifecycle boundary rather than instrumenting the production permit destructor.
- `GateableHooks` and `SafeRecordingDebugSink`: block or record hook and debug phases using URL, status, and header metadata only.

`DevBodyCaptureConfig` is a separate, deprecated, disabled-by-default local-file capture path. It persists raw selected response bytes to disk with no redaction, never captures request bodies, and skips protected auth-bearing requests and auth endpoint traffic by default. It is not a substitute for debug sinks or hooks, and tests should not use it to inspect secrets or production-like payloads.

Every harness wait is bounded through `wait_bounded`, `PhaseGate::wait_for`, or `PhaseGate::try_wait_for`. Tests that assert a task is still blocked may use a short bounded negative wait such as `assert_still_pending`, but the phase event must be the synchronization point. Do not make correctness depend on arbitrary wall-clock sleeps.

Harness event logs must remain safe metadata. They may include phase labels, sanitized URLs, statuses, and headers, but not request body bytes, response body bytes, raw auth material, or secret values. The harness self-tests live in `concord_core/tests/integration/current_core/async_harness.rs`; they prove blocking, release, drop observation, rate-limit, transport, body, hook ordering, bounded missing-phase waits, and safe observer surfaces.

The cancellation suite in `concord_core/tests/integration/current_core/cancellation.rs` reuses those helpers to prove that aborted rate-limit, hook, transport, body, and pagination work does not produce late semantic side effects. Timeout handling remains transport-delegated unless a runtime timer is explicitly documented elsewhere.

The concurrency characterization suite in `concord_core/tests/integration/current_core/concurrency.rs` uses the same helpers to prove that concurrent requests keep request-local config, rate-limit state, auth credential generations, pagination state, decode results, observer metadata, and cancellation outcomes isolated even when they reuse the same client, limiter, hooks, or debug sink.

## Examples And Docs Tests

`concord_examples` compile-checks public usage. It includes small examples, public docs fixtures, generated API usage tests, and the Riot fixture.

The Riot fixture is the large real-world surface test. Do not change Riot semantics for unrelated macro or runtime work.

Markdown prose is not validated by keyword tests in `cargo test`. Release review may include a manual source and documentation audit for outdated wording, unsafe live calls, or validation-dependent codegen patterns, but that audit is not a substitute for behavior, compile, diagnostic, and runtime tests.

## Full Local Gate

Run:

```bash
bash ./scripts/check_v1.sh
```

`scripts/check_v1.sh` requires `cargo-nextest` and performs:

```bash
bash ./scripts/check_architecture.sh
bash ./scripts/check_features.sh
cargo fmt --check
cargo clippy --workspace --all-targets
cargo nextest run -p concord_macros integration
cargo nextest run -p concord_macros generated
cargo nextest run -p concord_macros --test trybuild_current
cargo nextest run -p concord_macros --test trybuild_sema
cargo nextest run -p concord_macros --test trybuild_codegen
cargo nextest run -p concord_core
cargo nextest run -p concord_core --all-features
cargo nextest run -p concord_examples
cargo nextest run -p concord_examples --all-features
cargo nextest run --workspace
cargo nextest run --workspace --all-features
cargo nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The checked-in clippy step is non-strict (`cargo clippy --workspace --all-targets`). `-D warnings` is not enabled yet because the tree still carries known warnings in core and macros.

## New DSL Feature Checklist

1. Add or update parser AST.
2. Add parser success and fail fixtures.
3. Add semantic model fields.
4. Add sema resolution and diagnostics.
5. Add merge and inheritance behavior if applicable.
6. Add codegen.
7. Add core runtime support only if required.
8. Add public docs.
9. Add compiled example if public syntax.
10. Add dev docs if architecture changes.
11. Run full verification.
