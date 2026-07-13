# Test Ownership

This repository keeps test ownership split by layer so new coverage lands in the right place without duplicating the runtime matrices.

## Parser and normalization

- `concord_macros/src/parse/tests/` owns parser and raw-AST coverage.
- `concord_macros/src/sema/tests/normalize_*.rs` owns normalization and boundary-shape checks.

## Semantic analysis

- `concord_macros/src/sema/tests/auth_*.rs` owns auth semantics.
- `concord_macros/src/sema/tests/policy_*.rs` and `timeout_resolution.rs` own policy and timeout semantics.
- `concord_macros/src/sema/tests/behavior_*.rs` owns behavior resolution and inheritance.
- `concord_macros/src/sema/tests/route_*.rs` owns route resolution, inheritance, parameters, and diagnostics.
- `concord_macros/src/sema/tests/retry_*.rs` owns retry semantics.
- `concord_macros/src/sema/tests/rate_limit_*.rs` owns rate-limit semantics.
- `concord_macros/src/sema/tests/pagination_*.rs` owns pagination semantics.

## Structural codegen

- `concord_macros/src/codegen/tests/` owns generated-token and structural codegen assertions.

## Trybuild

- `concord_macros/tests/trybuild_current.rs` covers current pass fixtures grouped by public surface area.
- `concord_macros/tests/trybuild_sema.rs` covers parser and sema-facing failures grouped by diagnostic owner, including a separate route diagnostics wrapper for route/reference failures.
- `concord_macros/tests/trybuild_codegen.rs` covers codegen-contract and Rust type failures.

## Runtime

- `concord_core/tests/integration/current_core/` owns runtime behavior.
- `native_runtime.rs` owns managed-client execution, native response adaptation,
  timeout and task cancellation, request-limit preflight, and wire-level auth.
- `attempt_pipeline.rs`, `runtime_config.rs`, `runtime_order.rs`, and
  `redaction_matrix.rs` retain their P-05 behavioral matrices on the strict
  native loopback seam; `public_api.rs` owns the concrete-client positive
  surface while Trybuild owns removed-transport failures.
- `pagination.rs`, `retry_runtime.rs`, and `rate_limit.rs` retain their P-05
  behavioral matrices while executing through native loopback requests.
- Request recipe, exact-length, limiter, credential-cache concurrency,
  cancellation, error taxonomy, and redaction primitives are owned by focused
  unit modules in `concord_core/src/`.

### P-05 to P-06 ownership map

| P-05 suite responsibility | P-06 native owner |
| --- | --- |
| Authentication, refresh, invalidation, challenge, cache sharing | `native_runtime.rs`, generated `auth.rs`, `auth::credentials` and `auth::orchestrator` unit tests |
| Authentication concurrency and cancellation | `auth::credentials` unit tests plus gated native cancellation in `native_runtime.rs` |
| Status/connection retries and request rebuildability | `retry_runtime.rs`, generated `retry.rs`, and `io` recipe tests |
| Rate-limit admission, release, feedback, and ordering | `rate_limit.rs` and governor runtime unit tests |
| Hooks and response-classification ordering | `runtime_order.rs`, `rate_limit.rs`, and generated no-content/runtime tests |
| Pagination progression, caps, and non-progress | `pagination.rs` and generated `pagination.rs` |
| Buffered/stream response limits, errors, and cancellation | generated endpoint I/O modules, `body` unit tests, and `native_runtime.rs` |
| Streaming uploads, exact lengths, and global request limits | generated `endpoint_io_stream.rs`, `body` and `io` unit tests |
| Multipart construction, reconstruction, limits, and collisions | generated `endpoint_io_multipart.rs`, `io` and `multipart` unit tests |
| Request/response errors and complete redaction | `attempt_pipeline.rs`, `redaction_matrix.rs`, pagination/retry/rate-limit native suites, generated endpoint I/O/auth suites, and error/redaction unit tests |
| Runtime configuration propagation | `runtime_config.rs`, `native_runtime.rs`, generated endpoint I/O tests, and runtime config unit tests |
| Generated-client execution | `concord_macros/tests/integration/generated/` |
| Example-client execution | `concord_examples/tests/integration/` |
| Allocation, attempts, auth, pagination, hooks, streaming, smoke | native targets under `perf/benches/` |

### Inventory comparison

- The five high-density P-05 suites that had been removed are compiled again
  in their original ownership files: attempt pipeline (6), public API (3),
  redaction matrix (8), runtime configuration (14), and runtime ordering (61),
  for 92 restored native-loopback cases.
- The focused native runtime, pagination, retry, and rate-limit suites add 99
  integration cases. Together with output-model, request-entity, release-gate,
  and 202 core library tests, the all-feature core run contains 417 tests.
- Generated runtime integration remains 42 source-level test cases (P-05: 42),
  example integration remains 23 (P-05: 23), and performance remains nine
  benchmark targets (P-05: nine).
- Core integration source attributes are 213 rather than P-05's 420 because
  transport-fixture permutations were consolidated into native wire tests and
  focused unit owners. The table above records the replacement owner for each
  material responsibility; the reduction is not an empty module registry or a
  retired ownership area.

## Generated integration and examples

- `concord_macros/tests/integration/generated/` owns feature-owned generated-client integration against the strict native loopback runtime.
- `concord_examples/tests/` owns deterministic example checks. Live smoke paths stay opt-in behind environment variables.

## Feature and CI validation

- The root `justfile` owns the maintained workspace validation dimensions.
- `just release` is the canonical release gate and does not include deferred perf diagnostics.
- No-default, individual-feature, and dependency-tree checks are focused diagnostics, not proof supplied by the all-feature release check.
- Architectural boundaries are maintained through module/crate organization,
  compile-fail coverage, native runtime tests, and repository removal searches.
- `just perf-check`, `just perf-test`, and `just bench-check` validate the native
  performance fixtures and benchmark targets.

## Where to add new tests

- Put parser/raw-AST work in `concord_macros/src/parse/tests/`.
- Put semantic facts in the matching `concord_macros/src/sema/tests/` module for that feature area.
- Put generated-token shape checks in `concord_macros/src/codegen/tests/`.
- Put runtime behavior in `concord_core/tests/integration/current_core/`.
- Put feature-owned end-to-end generated-client checks in `concord_macros/tests/integration/generated/`.
- Put example behavior in `concord_examples/tests/`.
