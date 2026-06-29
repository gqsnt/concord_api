# Local V1 Release Gate

This gate is local workspace validation only. It does not package, publish, tag, or run any crates.io step. The default gate is deterministic, offline, and does not require credentials or network access.

Run:

```bash
bash ./scripts/check_v1.sh
```

`scripts/check_v1.sh` works from the repository root, uses `set -euo pipefail`, prints each step, and fails on the first failing command. It requires `cargo-nextest`.

## Command Matrix

The release command runs:

```bash
bash ./scripts/check_features.sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p concord_macros --test trybuild_current
cargo nextest run -p concord_macros --test main
cargo nextest run -p concord_core
cargo nextest run -p concord_examples
cargo nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The workspace command intentionally duplicates some package-level coverage. The package-level commands keep macro, core, and example failures visible; the workspace command catches cross-crate and all-target drift.

## Feature Compatibility

`scripts/check_features.sh` owns the feature and dependency matrix.

| Crate | v1 default features | Optional features | No-default support |
| --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart` | supported |
| `concord_macros` | none | none | supported |
| `concord_examples` | none | none | intentionally unsupported |

The feature script checks normal dependency trees, not dev-dependency trees, for the default feature story.

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

The check is compile-only. It proves imports and trait and type names remain available; behavioral compatibility is owned by the runtime and example suites.

## Examples Compatibility

`concord_examples` is the public usage fixture crate.

Current coverage:

- Basic generated GET, path parameters, `execute`, and `execute_decoded`:
  `concord_examples/tests/integration/minimal.rs`
- Auth placement and endpoint-backed credentials:
  `concord_examples/tests/integration/auth_session.rs`
- Retry and rate-limit policies:
  `concord_examples/tests/integration/policy_stack.rs`
- Pagination collect and per-page callbacks:
  `concord_examples/tests/integration/pagination.rs`
- Custom codecs and custom pagination controllers:
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

### page-mutation-before-auth-collision-rate-transport

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`, `pagination.rs`, and macro trybuild and codegen fixtures.

Page and custom request mutation happens before auth collision validation, rate-limit acquire, and transport materialization.

### url-host-path-hardening

Proof owners: `concord_examples/src/url_host_path.rs`, `concord_macros/tests/trybuild/fail/route/`, and `concord_core/src/types.rs` unit tests.

Base URLs reject dangerous forms, dynamic path segments reject `.`, `..`, `/`, and `\`, dynamic hosts are label-only, and static path strings remain trusted raw route fragments.

### body-limit-behavior

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`, `runtime_config.rs`, `cancellation.rs`, and `errors.rs`.

`Content-Length` over limit fails before body read. Unknown and chunked overflow fails during bounded read. Body-limit errors do not decode or map and do not retry as ordinary transport or status failures.

### feature-dependency-matrix

Proof owners: `scripts/check_features.sh` and `docs/features.md`.

Feature defaults and optional feature ownership remain explicit. Examples may use richer features than core or macros, but macro defaults must not enable runtime backends indirectly.

### runtimeconfig-defaults-precedence

Proof owners: `concord_core/tests/integration/current_core/runtime_config.rs` and `docs/runtime_config.md`.

Runtime defaults, client configuration, endpoint policy, pending overrides, and clone-on-write isolation are characterized.

### public-error-taxonomy-diagnostics

Proof owners: `concord_core/tests/integration/current_core/errors.rs` and `docs/errors.md`.

Public failures are matched by variant or category, not prose strings, and diagnostics remain body-free and auth-free.

### deterministic-async-harness

Proof owners: `concord_core/tests/integration/current_core/async_harness.rs` and `dev_doc/testing.md`.

Phase gates, drop probes, gateable rate, transport, body, and hook helpers, plus bounded waits, are deterministic and cancellation-safe. Harness helpers are test-only.

### timeout-cancellation-drop-semantics

Proof owners: `concord_core/tests/integration/current_core/cancellation.rs` and `dev_doc/core_runtime.md`.

Timeout is transport-delegated in v1. Cancellation by dropping or aborting request futures must not produce late decode, map, or page-advance side effects or poison later requests.

### concurrency-shared-state-isolation

Proof owners: `concord_core/tests/integration/current_core/concurrency.rs` and `dev_doc/core_runtime.md`.

Concurrent requests may interleave, but policy, auth identity, rate-limit key, body limit, timeout metadata, observer metadata, decode and map results, and pagination state remain request-local unless a test explicitly documents different behavior.

### execute-raw-bypass-contract

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`, `runtime_config.rs`, `errors.rs`, `cancellation.rs`, `concurrency.rs`, and `concord_examples/src/explicit_endpoint.rs`.

`execute_raw` uses validation, rate-limit, retry, and body-limit safety but bypasses endpoint decode and map.

### pagination-loop-snapshot-behavior

Proof owners: `concord_core/tests/integration/current_core/pagination.rs`, `runtime_config.rs`, `cancellation.rs`, `concurrency.rs`, and `docs/pagination.md`.

Pagination loop detection, config snapshots, page mutation order, and concurrent pagination state are deterministic and per run.

### semantic-ir-codegen-diagnostics

Proof owners: `concord_macros/src/sema/`, `concord_macros/src/codegen/`, and `concord_macros/tests/trybuild/`.

Codegen consumes resolved semantic IR. Invalid macro states are rejected by semantic diagnostics and trybuild fixtures rather than panics or raw AST assumptions.

### behavior-profile-semantic-only-sugar

Proof owners: `concord_macros/src/sema/`, `concord_macros/src/codegen/`, and behavior-profile snapshot tests.

Behavior and profile names are semantic-only policy sugar. Generated runtime code uses resolved policy and does not depend on behavior or profile names.

### endpoint-io-contract-current

Proof owners: `docs/advanced_endpoints.md`, `docs/customization.md`, `docs/retry_and_rate_limit.md`, `dev_doc/endpoint_io.md`, `dev_doc/architecture.md`, `concord_examples/src/endpoint_io.rs`, and `concord_examples/src/custom_codec.rs`.

The current endpoint I/O contract is documented as current behavior, not future work. `ContentType` is the shared wire-content marker, `Stream`, `Records`, `Multipart`, and `Sse` have generated support, explicit `Multipart<T, F>` and `Sse<T, C>` forms remain supported, stream-like request bodies are not automatically replayed, `map` and pagination remain buffered-response-only, and the core `NoContent` codec is distinguished from the unsupported reserved DSL `NoContent` and `Bytes` spellings.

## Known V1 Limitations

- `concord_examples --no-default-features` is intentionally unsupported.
- Ordinary endpoint requests are not in-flight coalesced. Credential acquisition and refresh may single-flight for the same credential slot.
- Timeout enforcement is transport-delegated unless a specific transport implements a timer.
- `execute_raw` is intentionally lower level and bypasses endpoint decode and map behavior.

## Adding Future Release Checks

Add checks to the narrowest owner first. Use compile-only public surface tests for API availability, trybuild for macro-facing diagnostics, integration tests for runtime behavior, and `scripts/check_features.sh` for feature and dependency surface drift. Then add the command or proof file to this document and make sure `scripts/check_v1.sh` invokes it directly or through an existing gate.
