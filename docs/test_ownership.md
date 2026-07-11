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
- The main suites are request/response entities, attempt pipeline, pagination, auth, retry, rate-limit, redaction, error taxonomy, transport, cancellation, concurrency, and streaming.

## Generated integration and examples

- `concord_macros/tests/integration/generated/` owns feature-owned generated-client integration against the mock runtime.
- `concord_examples/tests/` owns deterministic example checks. Live smoke paths stay opt-in behind environment variables.

## Feature and CI validation

- The root `justfile` owns the maintained workspace validation dimensions.
- `just release` is the canonical release gate and does not include deferred perf diagnostics.
- No-default, individual-feature, and dependency-tree checks are focused diagnostics, not proof supplied by the all-feature release check.
- Architectural boundaries are maintained through module/crate organization, targeted compile/runtime tests, and review; the historical source-regex audit is retired.
- `just perf-check`, `just perf-test`, and `just bench-check` are optional diagnostics for the historical perf package.

## Where to add new tests

- Put parser/raw-AST work in `concord_macros/src/parse/tests/`.
- Put semantic facts in the matching `concord_macros/src/sema/tests/` module for that feature area.
- Put generated-token shape checks in `concord_macros/src/codegen/tests/`.
- Put runtime behavior in `concord_core/tests/integration/current_core/`.
- Put feature-owned end-to-end generated-client checks in `concord_macros/tests/integration/generated/`.
- Put example behavior in `concord_examples/tests/`.
