# Testing

Concord uses several layers of tests because the project spans macro syntax, generated Rust, runtime behavior, and public docs.

## Macro tests

Trybuild pass/fail fixtures cover public macro UI contracts: downstream
compile boundaries, intended user-facing diagnostics, and span-sensitive
diagnostics. Fixtures are split by category under
`concord_macros/tests/trybuild/`.

The current trybuild test functions are:

- `trybuild_pass_contract_fixtures`
- `trybuild_auth_and_secret_diagnostics`
- `trybuild_route_and_fmt_diagnostics`
- `trybuild_policy_diagnostics`
- `trybuild_pagination_diagnostics`
- `trybuild_codegen_contract_diagnostics`

Run the full trybuild suite with:

```bash
cargo nextest run -p concord_macros --test trybuild_current
```

Refresh trybuild stderr output only when macro diagnostics intentionally
change:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current -- --test-threads=1
```

Category-specific refresh examples:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_auth_and_secret_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_route_and_fmt_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_policy_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_pagination_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_codegen_contract_diagnostics -- --test-threads=1
```

Only use `TRYBUILD=overwrite` when diagnostics intentionally change. Inspect
the git diff of `.stderr` files before accepting updates. Path-only changes
from fixture moves are acceptable; changed wording/spans must be reviewed.

Trybuild remains part of the full gate through
`cargo nextest run --workspace --all-targets`. It is serialized in
`.config/nextest.toml` with the `trybuild` test group because it drives many
rustc fixture compilations.

Parser unit tests cover smaller syntax rules and span-sensitive diagnostics.

Sema unit tests cover name resolution, inheritance, policy merging, behavior expansion, and diagnostics that need semantic context.

Codegen tests should prefer generated API compile checks, type checks, trybuild fixtures, and focused generated-shape assertions that cannot be expressed through Rust type checking.

Macro strictness belongs primarily in semantic unit tests and trybuild pass/fail fixtures. Add trybuild fail fixtures when a rejected form needs a stable public diagnostic. Source-level keyword audits can be useful during review, but they should not be normal `cargo test` checks.

Feature-surface drift is gated separately by `scripts/check_features.sh`. That script uses normal dependency trees for the crate-surface proof so dev-dependencies do not widen the default feature story. `scripts/check_v1.sh` calls it before the rest of the local gate.

## Core tests

`concord_core` has runtime characterization tests for cache, concurrency, rate-limit, auth rejection, retry, stale fallback, decode, pagination, codecs, and runtime configuration.

These tests protect runtime order and should be extended before runtime behavior is refactored.

Auth/redaction tests must cover arbitrary auth names, not only conventional names such as `Authorization` or `api_key`. Basic auth usernames declared as `secret` are secret material too. When auth handling changes, verify that `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, debug sinks, errors, and cache keys do not contain raw auth material, while the materialized `TransportRequest` still carries real credentials at `Transport::send`.

Auth preparation boundary tests should verify behavior at the sealed auth boundary: raw material stays out of logical request/debug/error surfaces and reaches only `TransportRequest` at send time.

Runtime strictness tests should reject invented policy values and silent saturation through observable behavior. Rate-limit `[host]` keys must fail explicitly when the logical URL has no host. Cache TTL overflow belongs in macro semantic diagnostics, and request/auth attempt counters should return typed overflow errors instead of saturating.

Runtime lock/state tests should poison representative auth, cache, and rate-limit state where feasible and assert typed errors or explicit cache backend outcomes instead of panics.

Response body limit tests should cover `Content-Length` precheck, unknown-length/chunked enforcement, exactly-at-limit success, decode/cache bypass on oversized bodies, auth HTTP token response limits, and separation between endpoint response read limits and cache `max_body`.

## Deterministic async harness

Runtime async, cancellation, and drop tests should use the test-only harness in
`concord_core/tests/integration/current_core/common.rs` instead of sleeps or
stress loops.

The shared helpers are:

- `PhaseGate`: records phase entry, waits for a phase count with a bounded
  timeout, blocks entrants, releases one waiter, releases all waiters, and
  preserves an ordered phase log. Release accounting is exact: duplicate
  releases do not create surplus permits for future entrants, and cancelled
  blocked waiters clean up their own accounting without leaking a future
  release.
- `DropProbe`: creates cloneable drop tokens, counts drops, and waits for a
  drop count with a bounded timeout. It also tries to log a labeled drop event
  when drop-time locking is available, but the count is the authoritative
  signal.
- `GateableTransport`: records transport send start and request metadata,
  blocks at `transport_send`, returns configured responses or transport
  errors, counts sends, and can attach a `DropProbe` to the in-flight send
  future.
- `GateableBodyTransport`: returns a streaming response body that blocks at
  `body_chunk`, counts chunks read, can produce deterministic partial reads,
  and can attach a `DropProbe` to the body stream.
- `CountingRateLimiter`: records acquire start/completion, permit creation,
  response observation, and deterministic lifecycle completion. The public
  runtime permit type is currently a unit value, so this helper records the
  observable lifecycle boundary rather than instrumenting the production
  permit destructor.
- `GateableCache`: blocks and records cache `before_request`,
  `after_response`, and `after_error` phases while preserving cache ordering
  assertions.
- `GateableHooks` and `SafeRecordingDebugSink`: block or record hook/debug
  phases using URL, status, and header metadata only.

Every harness wait is bounded through `wait_bounded`, `PhaseGate::wait_for`, or
`PhaseGate::try_wait_for`. Tests that assert a task is still blocked may use a
short bounded negative wait such as `assert_still_pending`, but the phase event
must be the synchronization point. Do not make correctness depend on arbitrary
wall-clock sleeps.

Harness event logs must remain safe metadata. They may include phase labels,
sanitized URLs, statuses, and headers, but not request body bytes, response body
bytes, raw auth material, or secret values. The harness self-tests live in
`concord_core/tests/integration/current_core/async_harness.rs`; they prove
blocking, release, drop observation, cache/rate/transport/body/hook ordering,
bounded missing-phase waits, and safe observer surfaces. The cancellation
suite in `concord_core/tests/integration/current_core/cancellation.rs` reuses
those helpers to prove that aborted cache, rate-limit, hook, transport, body,
stale-fallback, and pagination work does not produce late semantic side
effects. Timeout handling remains transport-delegated unless a runtime timer is
explicitly documented elsewhere.

## Examples and docs tests

`concord_examples` compile-checks public usage. It includes small examples, public docs fixtures, generated API usage tests, and the Riot fixture.

The Riot fixture is the large real-world surface test. Do not change Riot semantics for unrelated macro or runtime work.

Markdown prose is not validated by keyword tests in `cargo test`. Release review may include a manual source and documentation audit for stale wording, unsafe live calls, or validation-dependent codegen patterns, but that audit is not a substitute for behavior, compile, diagnostic, and runtime tests.

## Full local gate

Run:

```bash
bash ./scripts/check_v1.sh
```

`scripts/check_v1.sh` requires `cargo-nextest` and performs:

```bash
bash ./scripts/check_features.sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## New DSL feature checklist

1. Add or update parser AST.
2. Add parser success/fail fixtures.
3. Add semantic model fields.
4. Add sema resolution and diagnostics.
5. Add merge/inheritance behavior if applicable.
6. Add codegen.
7. Add core runtime support only if required.
8. Add public docs.
9. Add compiled example if public syntax.
10. Add dev docs if architecture changes.
11. Run full verification.
