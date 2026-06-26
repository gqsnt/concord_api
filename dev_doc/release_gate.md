# Local v1 Release Gate

This gate is local workspace validation only. It does not package, publish,
tag, or run any crates.io step. The default gate is deterministic, offline, and
does not require credentials or network access.

Run:

```bash
bash ./scripts/check_v1.sh
```

`scripts/check_v1.sh` works from the repository root, uses `set -euo pipefail`,
prints each step, and fails on the first failing command. It requires
`cargo-nextest`.

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

The workspace command intentionally duplicates some package-level coverage. The
package-level commands keep macro, core, and example failures visible; the
workspace command catches cross-crate/all-target drift.

## Feature Compatibility

`scripts/check_features.sh` owns the feature/dependency matrix.

| Crate | v1 default features | Optional features | No-default support |
| --- | --- | --- | --- |
| `concord_core` | `rate-limit-governor` | `json`, `gzip`, `brotli`, `deflate`, `cookies`, `multipart`, `cache-moka` | supported |
| `concord_macros` | none | none | supported |
| `concord_examples` | `cache-moka` | `cache-moka` | intentionally unsupported |

The feature script checks normal dependency trees, not dev-dependency trees, for
the default feature story. It verifies that `concord_macros` has no `[features]`
section and that its normal dependency tree does not pull in `moka`,
`http-cache-semantics`, or `serde_json`.

## Public v1 Surface

The compile surface is checked by
`concord_core/tests/integration/current_core/public_api.rs`.

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
CacheStore
CacheBefore
CacheAfter
CacheRevalidation
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

The check is compile-only. It proves imports and trait/type names remain
available; behavioral compatibility is owned by the runtime and example suites.

## Examples Compatibility

`concord_examples` is the public usage fixture crate. It intentionally enables
`cache-moka` by default and `json` through its `concord_core` dependency.

Current coverage:

- Basic generated GET, path parameters, `execute`, and `execute_decoded`:
  `concord_examples/tests/integration/minimal.rs`
- Auth placement and endpoint-backed credentials:
  `concord_examples/tests/integration/auth_session.rs`
- Cache, retry, and rate-limit policies:
  `concord_examples/tests/integration/policy_stack.rs`
- Pagination collect and per-page callbacks:
  `concord_examples/tests/integration/pagination.rs`
- Custom codecs and custom pagination controllers:
  `concord_examples/tests/integration/custom_codec.rs` and
  `concord_examples/tests/integration/custom_pagination.rs`
- Explicit endpoint/manual runtime usage and `execute_raw`:
  `concord_examples/src/explicit_endpoint.rs`
- Large generated API shape:
  `concord_examples/tests/integration/riot_large.rs`

`concord_core/tests/integration/current_core/release_gate.rs` checks that these
modules remain registered and that representative source anchors stay present.

## Invariant Checklist

Each invariant below has a stable anchor for lightweight documentation checks.

### cache-admission-after-endpoint-success

Proof owners: `concord_core/tests/integration/current_core/cache.rs`,
`runtime_order.rs`, `body.rs`, `errors.rs`, `cancellation.rs`, and
`concurrency.rs`.

Successful eligible responses are admitted to cache only after endpoint decode
and map/transform complete. Decode, map, body-limit, auth rejection,
cancellation, and transport/status failures do not admit successful cache
entries.

### body-auth-redaction-safety

Proof owners: `concord_core/tests/integration/current_core/redaction.rs`,
`body.rs`, `errors.rs`, `cancellation.rs`, `concurrency.rs`, and
`docs/errors.md`.

Request bodies, response bodies, raw auth, and secrets remain absent from
Display, Debug, source chains, debug sinks, hooks, rate-limit metadata, retry
metadata, and cache metadata.

### auth-cache-identity-partitioning

Proof owners: `concord_core/tests/integration/current_core/auth.rs`,
`cache.rs`, `runtime_order.rs`, and `concurrency.rs`.

Cache identity uses safe auth partitions and never raw credential values.
Protected requests without safe identity do not accidentally share cache.

### page-mutation-before-auth-collision-cache-rate-transport

Proof owners: `concord_core/tests/integration/current_core/runtime_order.rs`,
`pagination.rs`, and macro trybuild/codegen fixtures.

Page/custom request mutation happens before auth collision validation, cache
lookup, rate-limit acquire, and transport materialization.

### url-host-path-hardening

Proof owners: `concord_examples/src/url_host_path.rs`,
`concord_macros/tests/trybuild/fail/route/`, and
`concord_core/src/types.rs` unit tests.

Base URLs reject dangerous forms, dynamic path segments reject `.`, `..`, `/`,
and `\`, dynamic hosts are label-only, and static path strings remain trusted
raw route fragments.

### body-limit-behavior

Proof owners: `concord_core/tests/integration/current_core/body.rs`,
`runtime_config.rs`, `cancellation.rs`, and `errors.rs`.

`Content-Length` over limit fails before body read. Unknown/chunked overflow
fails during bounded read. Body-limit errors do not decode, map, retry as
ordinary transport/status failures, or cache-admit.

### feature-dependency-matrix

Proof owners: `scripts/check_features.sh` and `docs/features.md`.

Feature defaults and optional backend ownership remain explicit. Examples may
use richer features than core/macros, but macro defaults must not enable runtime
cache backends.

### runtimeconfig-defaults-precedence

Proof owners: `concord_core/tests/integration/current_core/runtime_config.rs`
and `docs/runtime_config.md`.

Runtime defaults, client configuration, endpoint policy, pending overrides, and
clone/COW isolation are characterized. Cache `max_body` and runtime body limit
remain separate.

### public-error-taxonomy-diagnostics

Proof owners: `concord_core/tests/integration/current_core/errors.rs` and
`docs/errors.md`.

Public failures are matched by variant/category, not prose strings, and
diagnostics remain body/auth/secret-free.

### deterministic-async-harness

Proof owners: `concord_core/tests/integration/current_core/async_harness.rs`
and `dev_doc/testing.md`.

Phase gates, drop probes, gateable cache/rate/transport/body/hooks, and bounded
waits are deterministic and cancellation-safe. Harness helpers are test-only.

### timeout-cancellation-drop-semantics

Proof owners: `concord_core/tests/integration/current_core/cancellation.rs`
and `dev_doc/core_runtime.md`.

Timeout is transport-delegated in v1. Cancellation by dropping/aborting request
futures must not produce late decode/map/cache-admission/page-advance side
effects or poison later requests.

### concurrency-shared-state-isolation

Proof owners: `concord_core/tests/integration/current_core/concurrency.rs` and
`dev_doc/core_runtime.md`.

Concurrent requests may interleave, but policy, auth identity, cache key,
rate-limit key, body limit, timeout metadata, observer metadata, decode/map
result, cache admission, and pagination state remain request-local unless a
test explicitly documents shared behavior.

### execute-raw-bypass-contract

Proof owners: `concord_core/tests/integration/current_core/cache.rs`,
`body.rs`, `runtime_config.rs`, `errors.rs`, `cancellation.rs`,
`concurrency.rs`, and `concord_examples/src/explicit_endpoint.rs`.

`execute_raw` uses validation/rate-limit/retry/body-limit safety but bypasses
endpoint cache lookup/store, decode, and map/transform.

### pagination-loop-snapshot-behavior

Proof owners: `concord_core/tests/integration/current_core/pagination.rs`,
`runtime_config.rs`, `cancellation.rs`, `concurrency.rs`, and
`docs/pagination.md`.

Pagination loop detection, config snapshots, page mutation order, and
concurrent pagination state are deterministic and per run.

### semantic-ir-codegen-diagnostics

Proof owners: `concord_macros/src/sema/`,
`concord_macros/src/codegen/`, and `concord_macros/tests/trybuild/`.

Codegen consumes resolved semantic IR. Invalid macro states are rejected by
semantic diagnostics and trybuild fixtures rather than panics or raw AST
assumptions.

### behavior-profile-semantic-only-sugar

Proof owners: `concord_macros/src/sema/`,
`concord_macros/src/codegen/`, and behavior-profile snapshot tests.

Behavior/profile names are semantic-only policy sugar. Generated runtime code
uses resolved policy and does not depend on behavior/profile names.

## Known v1 Limitations

- `concord_examples --no-default-features` is intentionally unsupported.
- Ordinary endpoint requests are not in-flight coalesced. Credential
  acquisition/refresh may single-flight for the same credential slot.
- Timeout enforcement is transport-delegated unless a specific transport
  implements a timer.
- `execute_raw` is intentionally lower level and bypasses endpoint
  decode/map/cache behavior.

## Adding Future Release Checks

Add checks to the narrowest owner first. Use compile-only public surface tests
for API availability, trybuild for macro-facing diagnostics, integration tests
for runtime behavior, and `scripts/check_features.sh` for feature/dependency
surface drift. Then add the command or proof file to this document and make sure
`scripts/check_v1.sh` invokes it directly or through an existing gate.
