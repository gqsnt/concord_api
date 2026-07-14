# Current Core test ownership

External integration tests in this directory exercise only supported public
surfaces. Runtime-plan and execution-pipeline invariants are crate-local under
`concord_core/src/regression_tests`, where tests have crate-private access
without widening `__private` or `advanced`. Generated-contract behavior stays
in `concord_macros/tests/integration/generated`, and explicit development
observations stay behind `__development`.

The adapters under `concord_core/src/regression_tests/test_api` are
crate-private test infrastructure. They let crate-local tests reach the
production preparation and execution pipeline without widening a public
surface; they are not a supported public request API. Production public and
generated request behavior is exercised by `public_context.rs`, the external
`concord_core/tests/public_extension.rs` target, and the macro-generated
integration fixtures. Deterministic harness setup is separate from the public
application types, while lifecycle ordering uses feature-gated
`__development` observations.

| Behavior area | Current owner and location |
| --- | --- |
| Native execution | Crate-local `src/regression_tests/native_runtime.rs`; deterministic scripts in `src/regression_tests/common.rs` and `concord_test_support/src/deterministic.rs` |
| Request bodies and exact length | Crate-local execution invariants in `native_runtime.rs` and `request_entities.rs`; public body constructors exercised through the crate-private adapter in `public_request_bodies.rs` |
| Authentication recovery | Crate-local `native_runtime.rs` and `runtime_order.rs`; public prepared endpoints in `tests/integration/current_core/public_context.rs`; generated policies in `concord_macros/tests/integration/generated/auth.rs` |
| Pagination | Crate-local `src/regression_tests/pagination.rs`; generated bindings in the macro integration and codegen suites |
| Rate limiting and `Retry-After` cooldown | Crate-local `src/regression_tests/rate_limit.rs` and `runtime_config.rs` |
| Runtime configuration | Crate-local `src/regression_tests/runtime_config.rs`; public construction/configuration boundaries in `public_context.rs` and external `tests/public_extension.rs` |
| Response limits | Crate-local `src/regression_tests/response_body_limit.rs` and `runtime_config.rs` |
| Request-error hooks | Crate-local `src/regression_tests/request_error.rs` and request-limit cases in `native_runtime.rs` |
| Redaction | Crate-local `src/regression_tests/redaction_matrix.rs`, with shared assertions from `tests/support/redaction.rs` |
| Lifecycle ordering | Crate-local `src/regression_tests/runtime_order.rs`; feature-gated observation surface tests under `__development` |
| Public extensions | External `tests/public_extension.rs`; application types import only `prelude` and `advanced`, with feature-gated deterministic support confined to harness setup |
| Generated status eligibility | `concord_macros/tests/integration/generated/retry_modes.rs` |

No external integration test constructs a Core runtime plan or imports broad
runtime aliases through `__private`.
