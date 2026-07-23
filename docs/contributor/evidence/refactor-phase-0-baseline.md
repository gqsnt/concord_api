# Concord refactor Phase 0 baseline

This is the directly observed baseline for the architectural refactor specified by
[`CONCORD_REFACTOR_REFERENCE.md`](../../CONCORD_REFACTOR_REFERENCE.md). It records the
starting state before product code changes. The reference remains authoritative.

## Repository and toolchain

- Date: 2026-07-23 (Europe/Paris).
- Branch: `master`.
- Starting HEAD: `bdc9717` (`temp doc`).
- Starting `git status --short`: empty.
- No repository-local `AGENTS.md` exists; the supplied personal instructions apply.
- Pinned toolchain: Rust/Cargo `1.97.1` (`x86_64-pc-windows-msvc`).
- Declared MSRV: Rust `1.85`.
- Available tools: `just 1.46.0`, `cargo-deny 0.20.2`,
  `cargo-nextest 0.9.140`, and `cargo-expand 1.0.123`.
- `cargo-clippy.exe` is not applicable to the pinned 1.97.1 toolchain, despite Clippy
  being named in `rust-toolchain.toml`.

The environment rejected remote/tag inspection because those repository operations
require separate authorization. Local history was inspectable. No claims about remote
branches or tags are made.

## Publication and compatibility evidence

Every current package manifest says `publish = false`. A crates.io query found:

- no package named `concord-core`;
- no package named `concord-macros`;
- no package named `concord-extras`;
- `concord` is already owned by an unrelated GPL Discord terminal client (latest
  observed version `2.4.4`).

There is no repository evidence of a published Concord API library or a maintained
downstream compatibility promise. The clean-break assumption for this `0.1.0`
workspace is therefore usable for implementation, but public crates.io publication
under the selected façade name is blocked until name ownership or registry policy is
resolved.

## Starting package and feature graph

Workspace members:

```text
concord_core
concord_macros
concord_test_support
concord_examples
concord_perf (path perf/)
```

Default members:

```text
concord_core
concord_macros
concord_examples
```

Internal product edges:

```text
concord_examples -> concord_core + concord_macros
concord_perf     -> concord_core + concord_macros + concord_test_support
concord_macros  -> concord_core (dev only) + concord_test_support (dev only)
concord_core    -> concord_test_support (optional normal edge and dev edge)
```

All internal version requirements are exactly `=0.1.0`.

Starting features:

```text
concord_core
  default = default-tls,http2,rate-limit-governor
  default-tls, http2, rate-limit-governor, json, records, csv, ndjson,
  multipart, gzip, brotli, deflate, dangerous-raw-response,
  dangerous-custom-retry, dangerous-dev-tools

concord_examples
  default = default-tls,http2,rate-limit-governor,json,multipart
  default-tls, http2, rate-limit-governor, json, multipart, records, csv,
  ndjson, dangerous-raw-response, dangerous-custom-retry, tokio

concord_test_support
  default = cargo-fixture,deterministic,loopback
  cargo-fixture, deterministic, loopback

concord_macros
  no features

concord_perf
  no features
```

The generated contract version is `14` in both
`concord_core/src/__private.rs`/`generated_contract` and
`concord_macros/src/codegen.rs`.

## Starting fixtures and lockfiles

Manifest inventory:

```text
concord_core/tests/fixtures/config_features/Cargo.toml
concord_macros/tests/fixtures/Cargo.toml
concord_macros/tests/fixtures/contract_mismatch/Cargo.toml
concord_macros/tests/fixtures/contract_mismatch/incompatible_core/Cargo.toml
concord_macros/tests/fixtures/custom_retry_disabled/Cargo.toml
concord_macros/tests/fixtures/custom_retry_enabled/Cargo.toml
concord_macros/tests/fixtures/development_tools_enabled/Cargo.toml
concord_macros/tests/fixtures/extension_certification/Cargo.toml
concord_macros/tests/fixtures/json_disabled/Cargo.toml
concord_macros/tests/fixtures/missing_core/Cargo.toml
concord_macros/tests/fixtures/multipart_disabled/Cargo.toml
concord_macros/tests/fixtures/no_governor_rate_limits/Cargo.toml
concord_macros/tests/fixtures/oauth_disabled/Cargo.toml
concord_macros/tests/fixtures/raw_response_disabled/Cargo.toml
concord_macros/tests/fixtures/renamed_core/Cargo.toml
```

Lockfiles:

```text
Cargo.lock
concord_core/tests/fixtures/config_features/Cargo.lock
concord_macros/tests/fixtures/Cargo.lock
```

## Starting example and documentation inventory

Cargo reports one library, one default binary, and these named example targets:

```text
authentication
body_workflows
dangerous_custom_retry
dangerous_raw_response
full_dsl_reference
generic_specifications
pagination
prepared_endpoint
rate_limiting
records
regional_api
riot
```

The only example integration test target is `canonical_examples`.

Documentation starts under `docs/{basic,advanced,concepts,developer,guides,reference}`.
The contributor command surface is the root `justfile`. It globally selects
`powershell.exe`, documents `python scripts/check.py` even though `scripts/` is absent,
and exposes `fmt`, `check`, `check-all`, `lint`, `test`, `test-all`,
`test-no-default`, `docs`, `examples`, `features`, `deny`, `bench`, and `verify`.
There are no checked-in GitHub workflow files.

## Starting public surface inventory

`cargo rustdoc -p concord_core --all-features -- -D warnings` generated 176 sidebar
items:

```text
enum=31
module=5
struct=118
trait=19
type=3
```

By documented module:

```text
concord_core=9
concord_core::__development=14
concord_core::advanced=120
concord_core::dangerous=3
concord_core::prelude=22
concord_core::records=8
```

Exact names:

```text
concord_core
  enum: PageLimit
  modules: __development, advanced, dangerous, prelude, records
  structs: PaginatedRequest, Request, ResponseRequest

concord_core::__development
  enums: BodyCategory, CapturedHeaderValue, Channel, ExecutionFailureKind,
    TransportFailure
  structs: ApplicationChannel, CapturedHeader, CapturedRequest,
    DeterministicExecutor, ExecutionFailure, ExpectedRequest, Gate,
    ProviderChannel, ScriptedResponse

concord_core::advanced
  enums: AuthErrorKind, BodyErrorKind, BodyKind, ChallengePolicy,
    ClientBuildErrorKind, CompletionOutcome, ConfigurationErrorKind,
    CooldownScope, CredentialReason, DebugLevel, ErrorCategory, LengthHint,
    PaginationErrorKind, ProviderBody, ProviderRetry, RateLimitErrorKind,
    RecoveryMode, RequestContentType, ResponseBodyErrorKind,
    SanitizedHeaderValue, StatusBodyState
  structs: ApiError, ApiResponse, Auth, AuthError, BeforeSend, BodyError,
    Buffered, Bytes, BytesMut, ByteStream, ClientBuildError, CodecError,
    CompletionEvent, ConfigurationError, Cooldown, CredentialHandle,
    CredentialRef, CredentialRequest, CursorPagination, DecodeContext,
    DefaultRateLimitObserver, Discard, EncodeContext, ExecutionMeta,
    GovernorRateLimiter, HeaderMap, HeaderName, HeaderValue, HookError,
    HttpConfigBuilder, HttpResponseFailure, Json, Method, Multipart,
    MultipartBody, MultipartPart, NoContent, NoopRateLimiter, OctetStream,
    OffsetPagination, OperationId, PagePagination, PaginationCompletionEvent,
    PaginationError, PreparedEndpoint, PreparedEndpointBuilder, ProviderHttp,
    ProviderRequest, ProviderResponse, RateLimitError, RateLimitId,
    RateLimitKey, RateLimitKeyPart, RateLimitPermit, RateLimitPlan,
    RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimitRule,
    RateLimitWindow, RequestBody, RequestBodyContext, RequestBodyMetadata,
    ResponseBodyError, ResponseHead, ResponseHeadEvent, RuntimeConfigBuilder,
    SafeProxy, SanitizedHeaderMap, SanitizedHeaders, StatusBody,
    StatusBodyDecodeError, StatusCode, Stream, StreamBody, Streaming, Text,
    Url, Version
  traits: BodyFactory, CredentialMaterial, CredentialProvider, DecodeBody,
    EncodeBody, HasNextCursor, MediaType, MultipartFactory, PageResponseSpec,
    PaginationController, ProviderBodyFactory, RateLimiter, RateLimitObserver,
    RequestBodySpec, ResponseBodySpec, ResponseMode, RuntimeHooks,
    SecretCredential
  aliases: BoxFuture, DecodedResponse

concord_core::dangerous
  structs: RawResponse, RawResponseRequest, ReqwestRetryBuilder

concord_core::prelude
  enums: DebugLevel, ErrorCategory, PageLimit
  structs: AccessToken, ApiError, ApiKey, ApiResponse, BasicCredential, Bytes,
    ByteStream, ClientBuildError, ConfigurationError, Json, Multipart,
    NoContent, OctetStream, RequestBody, SecretString, Stream, StreamBody, Text
  alias: DecodedResponse

concord_core::records
  enum: RecordErrorKind
  structs: Csv, NdJson, RecordBatches, RecordError, Records, RecordStream
  trait: RecordDecoder
```

Representative expanded signatures establish the legacy baseline:

```rust,ignore
pub fn health(&self) -> example_api::facades::health::HealthFacade;

pub fn item(
    &self,
    id: String,
) -> concord_core::Request<example_api::endpoints::health::Item>;

pub fn request<E>(&self, endpoint: E) -> concord_core::Request<E>
where
    E: example_api::ExampleApiEndpoint;

pub trait RootRequestExt {
    fn verbose(self, value: bool) -> Self;
    fn clear_verbose(self) -> Self;
}

pub trait ItemsRequestExt {
    fn paginate<L>(self, limit: L) -> concord_core::PaginatedRequest<Items>
    where
        L: Into<concord_core::PageLimit>;
}
```

Configured authentication currently retains the positional constructor:

```rust,ignore
pub fn new(
    tenant: String,
    api_secret: impl Into<concord_core::prelude::SecretString>,
    bearer_secret: impl Into<concord_core::prelude::SecretString>,
    username: impl Into<concord_core::prelude::SecretString>,
    password: impl Into<concord_core::prelude::SecretString>,
    client_id: impl Into<concord_core::prelude::SecretString>,
    client_secret: impl Into<concord_core::prelude::SecretString>,
    session_provider: impl CredentialProvider<Material = SessionToken>,
) -> Result<AuthApi, concord_core::prelude::ClientBuildError>;
```

Explicit endpoint setters are inherent, while bound request setters, pagination, and
endpoint credential acquisition are generated extension traits that require the
per-client prelude. Generated modules are split into `facades` and `endpoints`.

`PreparedEndpoint::builder(&'static str, Method)` is the current runtime-defined
endpoint entry point. Its private `DeadlineSelection` has three variants, but `build`
collapses `Inherit` and `Disabled` to the same `None` value. Raw response selection is
an inherent method on the generic core `Request<E>` behind private capability bounds
and the `dangerous-raw-response` feature.

## Size and expansion metrics

```text
Rust files: 271
Rust lines: 65,064
  concord_core: 167 files / 42,109 lines
  concord_macros: 80 files / 18,047 lines
  concord_test_support: 8 files / 2,236 lines
  concord_examples: 15 files / 2,207 lines
  perf: 1 file / 465 lines

Macro implementation: 8 files / 9,117 lines
  parse.rs: 2,453
  resolve.rs: 2,156
  codegen.rs: 3,757

`cargo expand -p concord_macros --test generated_surface`:
  7,254 lines
  409,352 characters
  25,302 whitespace-delimited lexical fragments
  581 declaration-shaped expanded lines
```

The expansion metric covers the existing multi-API `generated_surface` test, not a
single endpoint. It is retained only as a reproducible pre-refactor comparison point.

Baseline performance smoke (one iteration per category; Windows, warm release build):

```text
core client construction                                      229.8µs
generated client, facade, and request creation                  3.8µs
public head preparation                                        48.1µs
codec encoding and decoding                                     2.6µs
body-limit polling                                           2.8166ms
credential ready-path preparation and finalization             36.5µs
limiter ready-path acquisition                               2.5978ms
pagination loop overhead                                     2.3646ms
stream wrapping                                                 700ns
generic specification selection                                13.3µs
record framing                                                   800ns
```

This smoke run is observational and too small for statistical comparison.

## Invariant owners confirmed before extraction

| Invariant | Existing primary proof retained for migration |
|---|---|
| Construction is inert and clones share a frozen runtime | `concord_macros/tests/generated_surface.rs::request_creation_is_lazy_and_generated_client_clone_shares_runtime_configuration` |
| Native auth material stays private/redacted | `concord_core::execute::auth_tests::basic_and_custom_header_material_exist_only_at_the_native_boundary` plus core UI/privacy cases |
| First excess response bytes are withheld | `concord_core::execute::tests::buffered_decoder_never_observes_known_or_first_excess_response_bytes` |
| Pagination is sequential | `concord_core::pagination::runtime_tests::page_lifecycle_is_sequential_correlated_and_limited_once_per_page` |
| Pagination is all-or-error | `concord_core::pagination::runtime_tests::later_page_errors_keep_ordinary_categories_and_aggregate_status` |
| No-governor plans fail closed | `concord_macros/tests/compile.rs::governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter` |
| Completion follows result fixation | `concord_core::execute::tests::success_and_status_results_remain_fixed_while_completion_is_delayed` |
| Generated contract mismatch fails compilation | `concord_macros/tests/compile.rs::incompatible_core_contract_fails_during_compilation` |

All eight focused proofs passed on the baseline toolchain.

## Exact baseline commands and observed results

| Command | Result |
|---|---|
| `cargo metadata --format-version 1 --all-features` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS; Windows linker message warning for `concord_macros` |
| `cargo check --workspace --all-targets --all-features` | PASS; same linker warning and three unused-variable warnings in `examples/records.rs` |
| `cargo test --workspace` | FAIL; core runtime/unit tests reached by the command pass, but core Trybuild output differs from stored Rust diagnostics |
| `cargo test --workspace --all-features` | FAIL for the same core Trybuild diagnostic drift |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; pinned `cargo-clippy.exe` is not applicable |
| `cargo test --workspace --doc --all-features` | PASS; 4 compile-fail doctests |
| `cargo rustdoc -p concord_core --all-features -- -D warnings` | PASS |
| `cargo rustdoc -p concord_macros --all-features -- -D warnings` | PASS |
| `just verify` | FAIL/blocked at its Clippy step after both checks and formatting pass |
| `cargo test -p concord_macros --test generated_surface request_creation_is_lazy_and_generated_client_clone_shares_runtime_configuration -- --exact` | PASS |
| `cargo test -p concord_core --lib execute::auth_tests::basic_and_custom_header_material_exist_only_at_the_native_boundary -- --exact` | PASS |
| `cargo test -p concord_core --lib execute::tests::buffered_decoder_never_observes_known_or_first_excess_response_bytes -- --exact` | PASS |
| `cargo test -p concord_core --lib pagination::runtime_tests::page_lifecycle_is_sequential_correlated_and_limited_once_per_page -- --exact` | PASS |
| `cargo test -p concord_core --lib pagination::runtime_tests::later_page_errors_keep_ordinary_categories_and_aggregate_status -- --exact` | PASS |
| `cargo test -p concord_macros --test compile governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter -- --exact` | PASS |
| `cargo test -p concord_core --lib execute::tests::success_and_status_results_remain_fixed_while_completion_is_delayed -- --exact` | PASS |
| `cargo test -p concord_macros --test compile incompatible_core_contract_fails_during_compilation -- --exact` | PASS |
| `cargo test -p concord_core --test deterministic_redaction` | PASS |
| `cargo bench -p concord_perf --bench concord_overhead --no-run` | PASS |
| `cargo bench -p concord_perf --bench concord_overhead -- --smoke` | PASS; all 11 named categories executed |
| `cargo deny check` | PASS; warns about duplicate `core-foundation` and `windows-sys` versions |

The eight pre-existing Rust 1.97 Trybuild mismatches are:

```text
managed_reqwest_escape.rs
raw_sanitized_headers.rs
status_body_traits.rs
rate_limit_key_debug_absent.rs
rate_limit_opaque_values.rs
pagination_private_state.rs
request_spec_metadata_signature.rs
records_pagination_absent.rs
```

The semantic compiler failures still occur; only rendered diagnostics differ. No
Trybuild output was blessed in Phase 0.
