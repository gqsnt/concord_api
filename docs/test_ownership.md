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

- `concord_macros/tests/trybuild_current.rs` covers current pass fixtures.
- `concord_macros/tests/trybuild_sema.rs` covers parser/sema-facing failures.
- `concord_macros/tests/trybuild_codegen.rs` covers codegen-facing failures.

## Runtime

- `concord_core/tests/integration/current_core/` owns runtime behavior.
- The main suites are request/response entities, attempt pipeline, pagination, auth, retry, rate-limit, redaction, error taxonomy, transport, cancellation, concurrency, and streaming.

## Generated integration and examples

- `concord_macros/tests/integration/generated/` owns generated-client integration against the mock runtime.
- `concord_examples/tests/` owns deterministic example checks. Live smoke paths stay opt-in behind environment variables.

## Feature and CI scripts

- `scripts/check_v1.sh` owns the current v1 surface and ownership gates.
- `scripts/check_features.sh` owns the supported feature matrix.
- `scripts/audit_current.sh` owns public docs/examples audit checks and release hygiene checks.
- `scripts/check_architecture.sh` owns source-boundary and current architecture boundary checks.

## Where to add new tests

- Put parser/raw-AST work in `concord_macros/src/parse/tests/`.
- Put semantic facts in the matching `concord_macros/src/sema/tests/` module for that feature area.
- Put generated-token shape checks in `concord_macros/src/codegen/tests/`.
- Put runtime behavior in `concord_core/tests/integration/current_core/`.
- Put end-to-end generated-client checks in `concord_macros/tests/integration/generated/`.
- Put example behavior in `concord_examples/tests/`.
