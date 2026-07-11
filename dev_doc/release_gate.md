# Local V1 Release Gate

This gate is local workspace validation only. It does not package, publish, tag, or run any crates.io step. The gate does not require credentials or live service calls. Cargo dependency resolution and cargo-deny advisory data may require network access unless the necessary registry and advisory data are already cached.

Run the canonical maintained-workspace gate from the repository root:

```bash
just release
```

## Command Matrix

The release command runs one command for each maintained validation dimension:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-targets --all-features --no-tests fail --no-fail-fast --retries 0
cargo test --workspace --doc --all-features
cargo doc --workspace --no-deps --all-features
cargo deny check
```

The release path intentionally uses one maximal workspace axis for each
maintained dimension. Focused diagnostics remain available through the root
`justfile` but are outside the release dependency graph.

## Feature Compatibility

`just release` validates the combined all-feature workspace configuration.
No-default, individual-feature, and dependency-tree checks are focused
diagnostics to run when changing feature ownership or optional dependencies;
they are not part of the canonical release gate.

The historical source-regex architecture audit is retired. Architectural
boundaries are maintained through module and crate organization, targeted
compile/runtime tests, and review.

| Crate | v1 default features | Optional features | No-default support |
| --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart` | supported |
| `concord_macros` | none | none | supported |
| `concord_examples` | none | none | intentionally unsupported |

The feature table documents the intended ownership and supported configurations;
the focused commands above are diagnostic checks rather than release gates.

## Supply Chain Gate

Run the supply-chain policy gate separately when diagnosing it:

```bash
just supply-chain
```

This recipe requires `cargo-deny`. Install it with:

```bash
cargo install cargo-deny --locked
```

The gate checks advisories, yanked crates, license policy, dependency sources and registries, and the configured ban policy. It may require a cached advisory database or network access to refresh advisory data. It does not use live credentials.

The canonical release gate runs one executable-test axis:

```text
cargo nextest run --workspace --all-targets --all-features \
  --no-tests fail --no-fail-fast --retries 0
```

Focused default-feature, per-crate, UI, no-default, and feature-specific
commands are diagnostics and are not dependencies of `just release`.

## Public V1 Surface

The compile surface is checked by `concord_core/tests/integration/current_core/public_api.rs`.

The v1-facing core names include:

```text
ApiClient
ApiClientError
RuntimeConfig
DebugLevel
Endpoint
ClientContext
Transport
TransportRequest
TransportResponse
TransportBody
TransportError
TransportErrorKind
RateLimiter
RateLimitContext
RateLimitPermit
RateLimitResponseContext
RateLimitResponseAction
RuntimeHooks
DebugSink
AuthPlacement
AuthDecision
AuthError
PaginationTermination
HasNextCursor
PageItems
ResolvedPolicy
```

The check is compile-only. It proves imports and trait and type names remain available; behavioral stability is owned by the runtime and example suites.

## Examples Compatibility

`concord_examples` is the public usage fixture crate.

Current coverage:

- Basic generated GET, path parameters, `execute`, and `execute_decoded_with::<C>()`:
  `concord_examples/tests/integration/minimal.rs`
- Auth placement and endpoint-backed credentials:
  `concord_examples/tests/integration/auth_session.rs`
- Retry and rate-limit policies:
  `concord_examples/tests/integration/policy_stack.rs`
- Pagination collect-only `.paginate(...).collect().await` flow:
  `concord_examples/tests/integration/pagination.rs`
- Custom codecs and custom pagination:
  `concord_examples/tests/integration/custom_codec.rs` and
  `concord_examples/tests/integration/custom_pagination.rs`
- Explicit endpoint and manual runtime usage plus `execute_raw`:
  `concord_examples/src/explicit_endpoint.rs`
- Large generated API shape:
  `concord_examples/tests/integration/riot_large.rs`

`concord_core/tests/integration/current_core/release_gate.rs` checks that these modules remain registered and that representative source anchors stay present.

## Invariant Checklist

Each invariant below has a stable anchor for lightweight documentation checks.

### body-auth-redaction-safety

Proof owners: `concord_core/tests/integration/redaction.rs`, `errors.rs`, `cancellation.rs`, and `concurrency.rs`.

Request bodies, response bodies, raw auth, and secrets remain absent from `Display`, `Debug`, source chains, debug sinks, hooks, rate-limit metadata, and retry metadata.
Runtime hook order is fixed and not user-configurable. `pre_send` runs after rate-limit acquisition and before raw auth transport materialization, `post_response` runs after an HTTP response is received and before body read and endpoint decode, and `transport_error` only observes initial transport-send failures.
The deprecated dev body capture path is local-file-only, disabled by default, and writes raw selected response bytes without redaction; it never captures request bodies and is separate from debug sinks, hooks, stderr debug output, public errors, retry metadata, and rate-limit metadata.

### page-mutation-before-auth-collision-rate-transport

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`, `pagination.rs`, and macro trybuild and codegen fixtures.

Page and custom request mutation happens before final auth collision validation, rate-limit acquire, and transport materialization.

### url-host-path-hardening

Proof owners: `concord_examples/src/url_host_path.rs`, `concord_macros/tests/trybuild/fail/parser/route/`, `concord_macros/tests/trybuild/fail/sema/route/`, and `concord_core/src/types.rs` unit tests.

Base URLs reject dangerous forms, dynamic path segments reject `.`, `..`, `/`, and `\`, dynamic hosts are label-only, and static path strings remain trusted raw route fragments.

### body-limit-behavior

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`, `runtime_config.rs`, `cancellation.rs`, and `errors.rs`.

`Content-Length` over limit fails before body read. Unknown and chunked overflow fails during bounded read. Body-limit errors do not decode and do not retry as ordinary transport or status failures.

### feature-dependency-matrix

Proof owners: the maintained workspace check and `docs/features.md`.

Feature defaults and optional feature ownership remain explicit. Examples may use richer features than core or macros, but macro defaults must not enable runtime backends indirectly.

### runtimeconfig-defaults-precedence

Proof owners: `concord_core/tests/integration/current_core/runtime_config.rs` and `docs/runtime_config.md`.

Runtime defaults, client configuration, endpoint policy, pending overrides, clone-on-write isolation, and shared auth-state behavior are characterized.

### public-error-taxonomy-diagnostics

Proof owners: `concord_core/tests/integration/current_core/errors.rs` and `docs/errors.md`.

Public failures are matched by variant or category, not prose strings, and diagnostics remain body-free and auth-free.

### deterministic-async-harness

Proof owners: `concord_core/tests/integration/current_core/async_harness.rs` and `dev_doc/testing.md`.

Phase gates, drop probes, gateable rate, transport, body, and hook helpers, plus bounded waits, are deterministic and cancellation-safe. Harness helpers are test-only.

### timeout-cancellation-drop-semantics

Proof owners: `concord_core/tests/integration/current_core/cancellation.rs` and `dev_doc/core_runtime.md`.

Timeout is transport-delegated in v1. Cancellation by dropping or aborting request futures must not produce late decode or page-advance side effects or poison later requests.

### concurrency-shared-state-isolation

Proof owners: `concord_core/tests/integration/current_core/concurrency.rs` and `dev_doc/core_runtime.md`.

Concurrent requests may interleave, but policy, auth identity, rate-limit key, body limit, timeout metadata, observer metadata, decode results, and pagination state remain request-local unless a test explicitly documents different behavior.

### execute-raw-bypass-contract

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`, `runtime_config.rs`, `errors.rs`, `cancellation.rs`, `concurrency.rs`, and `concord_examples/src/explicit_endpoint.rs`.

`execute_raw` uses validation, rate-limit, retry, and body-limit safety but bypasses endpoint decode.

### pagination-loop-determinism

Proof owners: `concord_core/tests/integration/current_core/pagination.rs`, `runtime_config.rs`, `cancellation.rs`, `concurrency.rs`, and `docs/pagination.md`.

Pagination loop detection, per-run config state capture, page mutation order, and concurrent pagination state are deterministic.

### semantic-ir-codegen-diagnostics

Proof owners: `concord_macros/src/sema/`, `concord_macros/src/codegen/`, and `concord_macros/tests/trybuild/`.

Codegen consumes resolved semantic IR. Invalid macro states are rejected by semantic diagnostics and trybuild fixtures rather than panics or raw AST assumptions.

### behavior-profile-semantic-only-sugar

Proof owners: `concord_macros/src/sema/`, `concord_macros/src/codegen/`, and current behavior-profile structural tests.

Behavior and profile names are semantic-only policy sugar. Generated runtime code uses resolved policy and does not depend on behavior or profile names.

### endpoint-io-contract-current

Proof owners: `docs/advanced_endpoints.md`, `docs/customization.md`, `docs/retry_and_rate_limit.md`, `dev_doc/endpoint_io.md`, `dev_doc/architecture.md`, `concord_examples/src/endpoint_io.rs`, and `concord_examples/src/custom_codec.rs`.

The current endpoint I/O contract is documented as current behavior, not future work. `ContentType` is the shared wire-content marker; `Json<T>`, `Text<String>`, generic `Stream`, request-side `Multipart`, response-only `NoContent`, and response-only `Bytes` have generated support. Stream-like request bodies are not automatically replayed, pagination remains buffered-response-only, the core `NoContent` codec is distinguished from the DSL `-> NoContent` spelling, `-> NoContent` returns `()`, `-> Bytes` returns `bytes::Bytes` through the ordinary bounded buffered response path, and request-side `NoContent` and `Bytes` remain unsupported.

## Known V1 Limitations

- `concord_examples --no-default-features` is intentionally unsupported.
- Ordinary endpoint requests are not in-flight coalesced. Credential acquisition and refresh may single-flight for the same credential slot.
- Timeout enforcement is transport-delegated unless a specific transport implements a timer.
- `execute_raw` is intentionally lower level and bypasses endpoint decode behavior.

## Adding Future Release Checks

Add checks to the narrowest owner first. Use compile-only public surface tests for API availability, trybuild for macro-facing diagnostics, and integration tests for runtime behavior. Keep the canonical validation dimensions in the root `justfile` and avoid adding command-surface or inventory tests.
