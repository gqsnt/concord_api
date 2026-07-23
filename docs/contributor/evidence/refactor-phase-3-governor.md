# Phase 3 governor extraction and default composition validation

Phase 3 was implemented from commit `8c237e1` with Rust and Cargo 1.97.1.

## Resulting ownership and behavior

`GovernorRateLimiter` and `DefaultRateLimitObserver` now live only in
`concord_extras::rate_limit`. They implement documented public core contracts
and receive only semantic request plans, sanitized response heads, and the
centrally configured cooldown cap. The implementation retains finite quota and
cooldown capacities, idle pruning, bounded keys, checked arithmetic,
cancellation-safe waiting, and deterministic state eviction.

Core owns `RateLimiter`, `RateLimitObserver`, plans, keys, permits, cooldown
contexts, executor ordering, and the fail-closed fallback. A limiter advertises
its preparation bound through `RateLimiter::preparation_key_limit`; zero remains
the global-key-only setting. Core snapshots that bound with the immutable
runtime and validates a plan before credential, limiter, body, or transport
work. `RateLimitResponse::cooldown_cap` replaces the former governor-specific
task-local state.

`RuntimeConfigBuilder` distinguishes fallback, generated-default, and explicit
limiter selections. A non-empty generated plan fails with
`LimiterUnavailable` while the fallback is selected. An explicit limiter is
authoritative and prevents generated limiter or observer construction.

The façade owns default composition. Its `rate-limit-governor` feature enables
the extras implementation, and its hidden bridge installs that implementation
without making macros recognize or name it. The ordinary façade defaults still
include TLS, HTTP/2, and governor support. Core and extras feature-default
finalization remains assigned to Phase 5.

The generated-contract bridge advanced from 16 to 17. A transitional
direct-core observer preparation bridge remains solely to keep intermediate
macro tests green; it performs no observer construction or concrete limiter
selection and is explicitly scheduled for deletion with direct-core generation
in Phase 14.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo test -p concord_core --no-default-features` | FAIL after 247 unit tests and the preceding integration tests passed; the `dangerous_feature_ui` target has the same three Rust 1.97.1 diagnostic-format mismatches recorded in the Phase 0 baseline. No snapshot was blessed. |
| `cargo test -p concord_core --no-default-features --lib` | PASS; 247 tests |
| `cargo test -p concord_core --lib --all-features` | PASS; 329 lifecycle, authentication, rate-limit, pagination, records, redaction, and boundedness tests |
| `cargo test -p concord_extras --features rate-limit-governor` | PASS; 23 unit tests, pagination and public rate-limit integration tests, and the governor privacy doctest |
| `cargo test -p concord --features rate-limit-governor` | PASS; façade default injection, explicit precedence, lazy observer construction, and generated surface tests |
| `cargo test -p concord --no-default-features` | PASS; fail-closed generated plans and explicit precedence without governor |
| `cargo check -p concord_core --all-targets --all-features` | PASS |
| `cargo check -p concord_extras --all-targets --all-features` | PASS |
| `cargo test -p concord_macros` | PASS; all unit, compile/fixture, dependency-policy, generated runtime, UI, and doctest groups |
| `cargo test --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-no-governor-rate-limits --offline --locked` | PASS; generated and runtime endpoint plans fail closed unless an explicit or deliberate no-op limiter is supplied |
| `cargo test -p concord_core --test deterministic_redaction --all-features` | PASS |
| `cargo test --workspace --doc --all-features` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord_core --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord_extras --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS='-D warnings'; cargo doc -p concord --all-features --no-deps` | PASS; the localized pre-existing MSVC proc-macro linker message was emitted |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo +1.85.0 check -p concord_core --no-default-features` | PASS |
| `cargo +1.85.0 check -p concord_extras --no-default-features --features rate-limit-governor` | PASS |
| `cargo +1.85.0 check -p concord --no-default-features --features rate-limit-governor` | PASS |
| `cargo tree -p concord_core -e normal --depth 1` | PASS; no extras, façade, macros, or governor implementation edge |
| `cargo tree -p concord_extras -e normal --depth 1 --no-default-features --features rate-limit-governor` | PASS; core plus the intended HTTP/date/Tokio implementation dependencies |
| `cargo tree -p concord -e normal --depth 1 --no-default-features --features rate-limit-governor` | PASS; exactly core, extras, and macros |
| `cargo tree -p concord_macros -e normal --depth 1` | PASS; no core, extras, or façade runtime edge |

The no-governor fixture is deterministic and uses no external service.
Governor algorithm tests use paused Tokio time or explicit state transitions;
no sleep-based synchronization was added.

## Security and lifecycle evidence

Seeded rate-key, URL, header, credential, and response values remain absent
from hooks, completion events, debug output, errors, and limiter-independent
safe surfaces. The all-feature core suite proves credential preflight before
limiter acquisition, limiter acquisition before private credential
application, bounded recovery attempts, deterministic completion, sequential
pagination, cancellation behavior, and actual-key first-excess cutoff. Extras
tests prove cooldown proposals are clamped by the core-supplied cap.

## Diagnostics and bounded limitations

The exact full core no-default command is not reported as successful. Its only
observed failure is the three pre-existing Rust 1.97.1 Trybuild rendering
mismatches in the dangerous-feature absence target; the underlying compile
fail behavior remains correct. The broader eight-case core diagnostic drift
and unavailable pinned-toolchain Clippy remain the Phase 0 tooling blockers.

This phase changes bridge/runtime composition but does not add generated
endpoint items, so no new expansion or execution benchmark was claimed.
No selected design decision required a deviation.
