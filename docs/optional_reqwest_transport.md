# Optional Reqwest Transport Design Report

This is a report-only note that records the optional reqwest transport design and the implementation outcome in this branch. It does not change behavior itself.

## Implementation Outcome

This branch implements the design:

- `concord_core` now owns an optional `transport-reqwest` feature that keeps the default reqwest-backed client available on the default feature set.
- `reqwest` is now optional for `concord_core` and is absent from the `--no-default-features` dependency tree.
- `json` remains an independent core feature; it does not reintroduce `reqwest`.
- Generated clients use a feature-neutral `DefaultTransport` default generic path plus a marker-bound default-constructor surface, so custom transports remain available without reqwest.

Current report-only observations:

- `cargo tree -p concord_core` still shows the reqwest transport path on the default feature set.
- `cargo tree -p concord_core --no-default-features` omits `reqwest`.
- `cargo tree -p concord_core --no-default-features --features transport-reqwest` restores the reqwest transport path.
- The historical machine-local footprint report is retired. The combined
  all-feature workspace check remains part of `just release`; focused
  no-default and dependency-tree commands are diagnostics for feature changes.

## Current State Inventory

### Where `reqwest` is declared

- Workspace dependency: [`Cargo.toml`](../Cargo.toml)
- `concord_core` direct dependency with transport and multipart codec features: [`concord_core/Cargo.toml`](../concord_core/Cargo.toml)
- `concord_examples` no longer depends on `reqwest` directly: [`concord_examples/Cargo.toml`](../concord_examples/Cargo.toml)

`concord_test_support` does not depend on `reqwest` directly.

### Which crates depend on it

- `concord_core` depends on `reqwest` optionally for `ReqwestTransport`, default client construction, and HTTP codec integration when `transport-reqwest` is enabled; multipart request construction is also available through the `multipart` feature without requiring the transport feature.
- `concord_examples` no longer depends on `reqwest` directly.
- `concord_test_support` currently does not.

### Which modules expose or use the reqwest transport

- `concord_core/src/transport.rs`
  - defines `ReqwestTransport`
  - implements `Transport` for `ReqwestTransport`
  - converts `reqwest::Error` into `TransportError`
- `concord_core/src/client/api.rs`
  - `ApiClient<Cx, T>` defaults `T` to `ReqwestTransport`
  - `ApiClient::<Cx>::new(...)` constructs the default reqwest-backed client
  - `with_reqwest_builder(...)` configures a Concord-owned managed client
- `concord_core/src/lib.rs`
  - reexports `ReqwestTransport` through `advanced` and `prelude`
- `concord_core/src/client/mod.rs`
  - imports `ReqwestTransport` into the client module surface
- `concord_core/src/client/build.rs`, `execute.rs`, `send_flow.rs`, `auth_http.rs`
  - use the transport abstraction, but the default client path currently assumes the reqwest-backed transport type exists

### Which examples/tests/docs assume reqwest is available

Examples and tests:

- `concord_examples/Cargo.toml` enables `reqwest`
- `concord_examples/src/url_host_path.rs` and the other checked examples use `new_with_transport(...)` heavily and are transport-agnostic at the API level
- `concord_core/tests/integration/current_core/transport_contract.rs` uses the managed Reqwest builder path
- `concord_core/tests/integration/redaction.rs` exercises managed `ReqwestTransport` error redaction

Docs:

- [`docs/generated_client.md`](generated_client.md)
- [`docs/customization.md`](customization.md)
- [`docs/auth.md`](auth.md)
- [`docs/runtime_config.md`](runtime_config.md)
- [`docs/retry_and_rate_limit.md`](retry_and_rate_limit.md)
- [`docs/features.md`](features.md)

Those docs currently describe the default reqwest transport as part of the normal client story.

## Proposed Feature Shape

The likely feature name is `transport-reqwest`.

Recommended shape for the later implementation PR:

- `transport-reqwest` owns:
  - the `reqwest` dependency
  - `ReqwestTransport`
  - `ApiClient::<Cx>::new(...)`
  - `with_reqwest_builder(...)`
  - reqwest-specific helper exports
- default behavior:
  - `transport-reqwest` stays enabled by default so current callers keep working
  - the default client constructor and `ReqwestTransport` remain available on the default feature set
- `default-features = false` behavior:
  - `concord_core` still builds and exposes the transport abstractions, `with_transport(...)`, and custom transport integration
  - reqwest-specific convenience APIs are absent unless the feature is explicitly enabled

Interaction with existing features:

- `json` should continue to toggle the reqwest JSON integration, and should depend on the reqwest transport feature without assuming reqwest is always present.
- `rate-limit-governor` should remain independent.
- other HTTP codec features (`gzip`, `brotli`, `deflate`, `cookies`) should remain associated with the reqwest transport feature.
- `multipart` should remain independent from the transport feature and enable multipart request construction through reqwest multipart APIs.

## Compatibility Plan

The implementation PR should preserve the following:

- default users continue to get the same default client experience
- users who already build with `default-features = false` can still use:
  - `ApiClient::with_transport(...)`
  - custom transports
  - the transport abstractions in `concord_core::transport`
- examples/tests that require reqwest stay feature-correct by explicitly enabling the transport feature or by moving into reqwest-gated test targets
- generated API shape should not depend on reqwest except where the default transport convenience is explicitly selected

That means the implementation keeps `ApiClient::<Cx, ReqwestTransport>::new(...)` and provides a managed builder path, not caller-owned client injection.

## Future PR 16B Implementation Plan

Concrete steps for the implementation PR:

1. Add a `transport-reqwest` feature in `concord_core/Cargo.toml`.
2. Move the `reqwest` dependency and reqwest-specific feature flags under that feature.
3. Gate `ReqwestTransport` and `with_reqwest_builder(...)` behind the transport feature.
4. Keep `Transport`, `http::Request<DynBody>`, `http::Response<DynBody>`, `with_transport(...)`, and custom transport support always available.
5. Update any default transport aliases or generated-client convenience constructors so they compile both with and without the feature.
6. Split or gate tests that directly exercise reqwest transport behavior.
7. Update docs/examples that mention the default reqwest transport.
8. Re-run:
   - `cargo check -p concord_core`
   - `cargo check -p concord_core --no-default-features`
   - `cargo check -p concord_core --no-default-features --features json`
   - `cargo test --workspace --all-targets`
   - `just perf-check`, `just perf-test`, and `just bench-check` for deferred diagnostics

Expected migration notes:

- callers using the default client keep their current behavior by staying on the default feature set
- callers using only custom transports can opt out of reqwest and keep the core transport abstractions
- the implementation PR should explain that `json` and the optional HTTP codec features now depend on the reqwest transport feature

## Risk Analysis

- Accidental default-feature breakage:
  - the default client constructor and generated default transport path are part of the public ergonomics surface, so they need a compatibility wrapper
- Docs/examples failing under `--no-default-features`:
  - current docs and tests assume reqwest exists, so they need gating or alternate examples in the implementation PR
- Tests relying on reqwest transport:
  - transport-contract and redaction tests currently use reqwest-specific behavior and need explicit gating
- Semver/public API concerns:
  - removing `ReqwestTransport` from the default surface without a replacement would be a breaking change
  - changing the generated client default type parameter would also be user-visible
- Feature unification pitfalls:
  - workspace-level feature unification can accidentally re-enable reqwest if another crate pulls it in directly
  - the implementation PR should keep crate boundaries explicit and check dependency trees
- CI/check matrix expansion:
  - the feature matrix should cover default on/off and the reqwest-transport-specific path
- Benchmark/report interpretation:
  - footprint improvements should be measured against the same workspace and lockfile, and reported as machine-local timing/size observations only

## Measurement Plan

Use these commands in the implementation PR to compare before and after:

```bash
cargo tree -p concord_core
cargo tree -p concord_core --no-default-features
cargo check -p concord_core
cargo check -p concord_core --no-default-features
cargo check -p concord_core --no-default-features --features json
just perf-check
```

If the implementation PR adds feature-specific test targets or supply-chain checks, include those in the same before/after comparison.

## Acceptance Criteria For PR 16B

The implementation PR should not be merged until all of the following are true:

- default-feature builds still expose the current reqwest-backed convenience path
- `concord_core --no-default-features` builds and passes tests that are meant to stay transport-agnostic
- reqwest-dependent APIs, docs, and tests are gated or moved appropriately
- custom transports continue to work without reqwest
- workspace tests pass
- the footprint report is updated and reviewed
- there are no runtime/auth/retry/rate-limit behavior changes
- no generated API shape changes leak into callers that did not opt into reqwest-specific behavior

## Non-Goals

This report does not:

- change feature flags
- gate any module
- remove or add dependencies
- change runtime behavior
- change generated APIs
- change the transport trait surface
- change tests or docs beyond this report note
