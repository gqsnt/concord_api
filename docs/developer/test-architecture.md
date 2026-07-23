# Concord test architecture

This document is the ownership map for Concord's proofs. It records both the
Phase 12 starting inventory and its final disposition, and remains the
contributor guide for deciding where a new test belongs.

The governing rule is: use the lowest semantic layer that can prove the claim.
A compiler failure proves an unavailable method or private boundary; a runtime
test proves ordering and behavior; a separate Cargo package is justified only
when its resolved dependency or feature graph is the subject of the proof.

## Primary invariant owners

| Product invariant | Primary owner and proof |
|---|---|
| Predictable generated names and scope topology | `concord` generated-surface integration tests, supplemented by macro surface-model unit tests |
| Order-independent clauses | `concord_macros` syntax permutation unit tests |
| Duplicate clause spans and reference/collision diagnostics | `concord_macros` parser/resolver unit tests plus categorized `concord` UI cases |
| Macro type neutrality and extras agnosticism | `concord` third-party-type compile/runtime tests plus workspace dependency metadata |
| One dependency and renamed façade | `concord` public compile tests plus `fixtures/dependency/renamed_facade` |
| Inherent optional/default methods and capability absence | `concord` surface assertions, rustdoc smoke, and UI compile failures |
| Cross-client endpoint rejection and generated privacy | `concord` UI compile failures |
| Endpoint/request construction is inert | `concord_core` lifecycle tests; generated equivalence is checked in `concord` |
| Immutable built runtime and clone sharing | `concord_core` lifecycle tests; generated clone behavior is checked in `concord` |
| Hook/body/auth/limiter ordering | `concord_core` deterministic lifecycle tests |
| Native request/authentication privacy | `concord_core` security runtime and UI tests |
| One bounded authentication recovery | `concord_core` authentication lifecycle tests |
| First excess is withheld for every acquisition mode | `concord_core` bounds tests |
| Redaction across errors, events, plans, descriptions, and debug output | `concord_core` seeded security matrix; extras add implementation-specific sentinels |
| Pagination is sequential, all-or-error, bounded, and cycle-safe | `concord_core` pagination engine tests |
| Built-in pagination controller policy | `concord_extras` controller tests |
| Governor absence fails closed | `fixtures/features` no-governor runtime proof |
| Governor state, keys, cooldowns, and permits remain bounded | `concord_extras` governor tests |
| OAuth2 protocol behavior and redaction | `concord_extras` provider tests; `concord` proves generated-provider lifecycle composition |
| Runtime and generated endpoints share one executor | `concord` deterministic generated/runtime integration test |
| Timeout inheritance, explicit duration, and disable are distinct | `concord_core` runtime tests plus `concord` planning tests |
| Feature-disabled public capabilities are absent | `fixtures/features` |
| Examples are canonical | `concord_examples` tests importing the actual modules |
| Public documentation and paths stay current | doctests, `concord` rustdoc smoke, and documentation link checks |
| Compile/runtime regressions remain measured | `tools/perf` harness and recorded Phase 14 baselines |

Support infrastructure is tested by `concord_test_support` itself. A test of a
script queue, gate, or redacted fixture diagnostic is not a core-runtime test
merely because core consumes that helper.

## Phase 12 starting inventory and disposition

The tables below name every integration test file present at the Phase 12
starting commit (`39cbf9d`). “Move” means the assertion is preserved at its
proper owner before the old file is deleted. “Merge” means the exact assertions
remain but share a contract-oriented harness.

### `concord_core`

| Starting file | Classification | Phase 12 disposition |
|---|---|---|
| `authentication_api.rs` | Public semantic auth contracts | Merge under `public_types` |
| `body_extensions.rs` | Third-party request-body contract | Merge under `public_types`; façade public-extension parity remains a separate external proof |
| `codec_extensions.rs` | Third-party buffered codec contract | Merge under `public_types` |
| `compression_features.rs` | Dependency feature isolation | Move to the root feature/dependency matrix |
| `config_methods.rs` | Standalone feature-absence driver | Replace with `fixtures/features`, then delete |
| `dangerous_feature_ui.rs` | Feature-combination compiler privacy | Merge into the core UI harness |
| `dangerous_features.rs` | Enabled dangerous core API signatures | Merge under `configuration`; absence remains fixture/UI-owned |
| `deterministic_boundary.rs` | Deterministic support executor contract | Move to `concord_test_support` |
| `deterministic_redaction.rs` | Test-support diagnostic redaction | Move to `concord_test_support` |
| `development.rs` | Development-feature fixture driver | Replace with `fixtures/features`, then delete |
| `error_api.rs` | Structured public errors | Merge under `public_types` |
| `error_redaction.rs` | Recursive error redaction | Merge under `security` |
| `generated_contract.rs` | Hidden generated-contract handshake | Merge under `public_types`; replacement-graph mismatch remains a fixture |
| `hook_redaction.rs` | Hook error redaction | Merge under `security` |
| `hooks_api.rs` | Public hook contracts | Merge under `public_types` |
| `http_config.rs` | HTTP builder defaults/signatures | Merge under `configuration` |
| `https_auth_contract.rs` | TLS/auth boundary and redaction | Merge under `lifecycle` |
| `multipart_extensions.rs` | Public semantic multipart factory | Merge under `public_types` |
| `multipart_feature.rs` | Reqwest feature isolation | Move to the root dependency matrix |
| `operation_identity.rs` | Opaque identity traits | Merge under `public_types` |
| `pagination_api.rs` | Generic pagination extension contracts | Merge under `public_types`; concrete policies remain extras-owned |
| `property/safe_url.rs` | Protected URL projection properties | Retain as a private redaction property module included from core source tests |
| `proxy.rs` | Safe proxy projection | Merge under `transport` |
| `public_rustdoc.rs` | Public path/privacy/doc assertions | Move to the façade rustdoc owner and focus on stable product paths |
| `rate_limit_api.rs` | Generic limiter/observer contracts | Merge under `public_types`; no concrete governor is used |
| `request_wrappers.rs` | Core request wrapper signatures | Merge under `public_types` |
| `reqwest_contract.rs` | Native transport policy and lifecycle | Merge under `transport` without changing deterministic assertions |
| `runtime_config.rs` | Finite runtime configuration | Merge under `configuration` |
| `runtime_hooks.rs` | Runtime hook configuration | Merge under `configuration` |
| `sanitized_metadata.rs` | Sanitized header/URL public surface | Merge under `security` |
| `security/redaction_matrix.rs` | Seeded redaction vocabulary matrix | Retain as the primary security matrix module |
| `status_body_api.rs` | Complete-only status body API | Merge under `public_types` |
| `status_body_contract.rs` | Bounded status acquisition | Merge under `bounds` |
| `stream_response_api.rs` | Bounded stream public API | Merge under `public_types` |
| `ui.rs`, `ui_dangerous.rs`, `ui_multipart_disabled.rs` | Core privacy, sealed-mode, and feature UI | Consolidate into one feature-aware core UI harness |

The core UI fixtures remain core-owned because they prove native/private
semantic boundaries rather than generated-language diagnostics:

- authentication material: `authentication_private_material`;
- dangerous/private namespaces: `dangerous_namespace_escape`,
  `development_namespace_escape`, `dangerous_dev_dangerous_namespace_escape`,
  `dangerous_dev_stable_namespace_escape`;
- dangerous feature absence and separation:
  `dangerous_custom_retry_absent`,
  `dangerous_custom_retry_absent_with_raw_response`,
  `dangerous_raw_response_absent`,
  `dangerous_raw_response_absent_with_custom_retry`,
  `dangerous_development_absent`;
- dangerous bounded response privacy: `dangerous_raw_response_fields_private`;
- native transport and URL privacy: `managed_reqwest_escape`,
  `proxy_raw_url_escape`;
- multipart feature absence: `multipart_feature_absent`,
  `multipart_spec_feature_absent`;
- generic pagination/records privacy: `pagination_private_state`,
  `records_pagination_absent`;
- limiter semantic privacy: `rate_limit_context_exclusions`,
  `rate_limit_key_debug_absent`, `rate_limit_opaque_values`;
- sanitized/status body privacy: `raw_sanitized_headers`,
  `status_body_traits`;
- extension-contract bounds and sealed acquisition:
  `request_spec_metadata_signature`, `request_spec_non_send_input`,
  `response_mode_is_sealed`.

### `concord_macros`

Parser, syntax normalization, resolver, surface-model, and emission unit tests
under `src/` remain in the proc-macro package. The starting integration files
have these dispositions:

| Starting file | Classification | Phase 12 disposition |
|---|---|---|
| `compile.rs` | Mixed fixture, rustdoc, public extension, and source-scan driver | Split: graph checks become façade public-compile tests; direct extension/runtime checks become façade integration tests; source scans and enabled standalone packages are deleted after compiler/runtime replacements |
| `dependency_policy.rs` | Workspace metadata/dependency policy | Move to façade/test tooling ownership and add a direct extras metadata assertion |
| `generated_authentication.rs` | Generated public auth API/runtime | Move to `concord` |
| `generated_pagination.rs` | Generated public pagination API/runtime | Move to `concord` |
| `generated_prepared_endpoints.rs` | Runtime endpoint lifecycle/capabilities | Move to `concord` and use final runtime-endpoint terminology |
| `generated_rate_limiting.rs` | Generated plan/default/custom-limiter runtime | Move to `concord` |
| `generated_raw_response.rs` | Generated dangerous terminal/bounds | Move to feature-gated `concord` integration |
| `generated_request_shaping.rs` | Generated values/head/default/timeout behavior | Move to `concord` |
| `generated_surface.rs` | Generated bodies/responses/modules/laziness | Move to feature-gated `concord` integration |
| `macro_type_neutrality.rs` | Supplemental broad source scan | Delete after metadata and third-party-type compiler proofs are in place |
| `ui.rs` and `ui/**` | Public language/capability diagnostics | Move and categorize under `concord/tests/ui` |
| `design/target_public_contract.md` | Historical test-local design copy | Delete after durable docs link to the authoritative reference and public API guide |
| `support/mod.rs` | Test-local codec/controller implementations | Move beside façade generated tests; import only `concord` |

The macro UI cases move to the façade in these ownership groups:

- malformed syntax: `malformed`, `invalid_custom_method`, `legacy_var`,
  `child_inside_client`, `root_client_config`, `second_client`,
  `rate_limit_removed_syntax`;
- names and references: `ambiguous_scope_reference`, `normalized_collision`,
  `unknown_public_reference`, `missing_scope_value`;
- authentication: `authentication_credential_ref_mutation_absent`,
  `authentication_custom_material_incompatible`,
  `authentication_custom_set_absent`;
- pagination: `pagination_bounds`, `pagination_method_absence`,
  `pagination_modes`, `pagination_syntax`, `pagination_types`,
  `stream_pagination_absent`;
- privacy: `arbitrary_endpoint`, `forged_endpoint`,
  `generated_executor_escape`, `named_request_fields_private`;
- capabilities/types: `multipart_response`,
  `no_content_request_spec`, `response_only_request_spec`,
  `wrong_endpoint_setter_absent`, `vector_public_header`; the duplicate
  `cross_client_endpoint` case was removed in favor of the existing façade
  `wrong_client_endpoint` proof;
- runtime endpoints and dangerous capabilities:
  `dangerous_raw_runtime_endpoint_absent`,
  `runtime_endpoint_builder_capabilities_absent`,
  `runtime_endpoint_fields_private`,
  `runtime_endpoint_not_in_prelude`,
  `runtime_endpoint_pagination_absent`; the duplicate macro
  `dangerous_raw_stream_rejected` case was removed in favor of the façade
  `raw_stream_mode_rejected` proof;
- rate limits: `rate_limit_invalid_cost`, `rate_limit_off_exclusive`,
  `rate_limit_optional_dimension`, `rate_limit_secret_dimension`,
  `rate_limit_undefined_attachment`;
- compile-pass language: `client_clause_order_independent`,
  `endpoint_clause_order_independent`, `rate_limit_no_display`,
  `request_setters_inherent`.

Final names replace “prepared” in active test paths. No test preserves the
removed generated prelude, old OAuth syntax, old retry DSL, PascalCase source
identifiers, or fixed clause ordering. The single
`prepared_endpoint_alias_deprecated` compatibility case remains explicitly
scheduled for removal with the alias in Phase 14.

### `concord`

| Starting file | Classification | Phase 12 disposition |
|---|---|---|
| `deterministic_generated_runtime.rs` | Direct/explicit lifecycle equivalence | Retain as primary generated-runtime proof |
| `endpoint_planning.rs` | Description, zero-effect plan, runtime parity | Retain |
| `generated_surface.rs` | Named wrapper/module/endpoint signatures | Retain and supplement with moved macro-owned suites |
| `oauth2_provider.rs` | Generated provider lifecycle integration | Retain; concrete protocol policy remains extras-owned |
| `public_namespaces.rs` | Stable façade paths/features | Retain as the focused namespace signature proof |
| `rate_limit_composition.rs` | Façade default and explicit precedence | Retain |
| `type_neutrality.rs` | Unknown/aliased type compiler proof | Retain and supplement with public extensions |
| `ui.rs` and `ui/**` | DSL v2 and capability diagnostics | Retain, categorize, and merge moved macro cases |

The existing façade UI cases remain final proofs, categorized as:

- construction/auth: `configured_client_no_new`,
  `secret_requires_explicit_wrapper`;
- DSL syntax/names: `custom_retry_dsl_removed`, `duplicate_clause`,
  `duplicate_contract_id`, `inferred_body`, `invalid_body_attribute`,
  `multiple_bodies`, `pascal_source_name`;
- capabilities/privacy: `non_clone_body_endpoint`,
  `non_paginated_no_paginate`, `raw_stream_mode_rejected`,
  `storage_terminal_source_only`, `wrong_client_endpoint`;
- Phase 14 deletion proof: `prepared_endpoint_alias_deprecated` is retained
  only until the alias itself is removed, then replaced by an ordinary
  unresolved-name compile failure if useful;
- compile pass: `configured_client_builder`, `dsl_v2`, `named_surface`,
  `runtime_endpoint`.

### `concord_extras`

| Starting file/test family | Classification | Phase 12 disposition |
|---|---|---|
| `tests/pagination.rs` | Public built-in controller contracts | Retain as `pagination` owner |
| `tests/rate_limit.rs` | Public governor/observer contract | Rename/group as governor public-contract coverage |
| `src/codec/json.rs` tests | JSON media/encode/decode/safe errors | Retain locally; add external public-contract signatures |
| `src/records/csv.rs` tests | CSV framing/options/errors | Retain locally |
| `src/records/ndjson.rs` tests | NDJSON framing/errors/media | Retain locally |
| `src/rate_limit/governor.rs` tests | Governor bounds/concurrency/cooldown | Retain locally |
| `src/rate_limit/observer.rs` tests | Retry-After policy | Retain locally |
| `src/auth/oauth2_client.rs` tests | OAuth2 URL/form/decode/expiry/redaction | Retain locally |

An extras metadata integration test is added to prove there is no façade or
macro dependency. The package is intentionally tested directly against
`concord_core` public contracts, never through a friend module.
Public integration coverage lives in `tests/json.rs`, `tests/records.rs`,
`tests/pagination.rs`, `tests/governor.rs`, `tests/oauth2.rs`, and
`tests/public_contract.rs`; detailed framing and protocol edge cases remain
near their private implementation helpers as unit tests.

### Examples and support

| Starting file | Classification | Disposition |
|---|---|---|
| `concord_examples/tests/construction.rs` | Canonical module construction | Retain |
| `concord_examples/tests/local_runtime.rs` | Canonical deterministic behavior | Retain |
| `concord_examples/tests/riot_contract.rs` | Actual Riot declarations/plans | Retain |
| core `deterministic_boundary.rs` | Support executor behavior | Move to `concord_test_support/tests/deterministic.rs` |
| core `deterministic_redaction.rs` | Support diagnostic redaction | Merge into the same support-owned test |

## Fixture inventory and graph-dependent reasons

The two starting fixture workspaces are replaced by one root `fixtures`
workspace and one lockfile.

| Starting fixture | Decision and exact replacement |
|---|---|
| core `config_features` | Merge absence bins into `fixtures/features`; enabled methods are covered by normal package matrices |
| `renamed_facade` | Keep at `fixtures/dependency/renamed_facade`; dependency renaming genuinely needs a separate extern-prelude graph |
| `contract_mismatch` + `incompatible_core` | Retain as `fixtures/dependency/contract_mismatch` with a deliberately incompatible façade bridge package; package replacement genuinely needs a separate graph |
| `custom_retry_disabled` | Merge into `fixtures/features` |
| `custom_retry_enabled` | Delete after façade all-features surface compilation |
| `development_tools_enabled` | Delete after workspace all-features compilation |
| `extension_certification` | Delete after its types and runtime assertions compile/run in `concord::public_extensions` |
| `json_disabled` | Merge into `fixtures/features`; also prove NDJSON does not unify JSON |
| `missing_core` | Delete: direct proc-macro consumption is not a supported product graph |
| `multipart_disabled` | Merge into `fixtures/features` |
| `no_governor_rate_limits` | Merge its deterministic fail-closed runtime test into `fixtures/features` |
| `oauth_disabled` | Merge into `fixtures/features` |
| `raw_response_disabled` | Merge into `fixtures/features` |

`fixtures/features` also owns CSV, NDJSON, records, TLS, HTTP/2, gzip,
Brotli, and deflate absence. Each check is invoked in an isolated Cargo
command so feature unification from one bin cannot make another absence proof
pass accidentally. Fixture build output is redirected beneath
`target/fixtures`; fixtures are not linked into the main rust-analyzer
workspace.

Contributors actively editing fixture sources can temporarily add
`fixtures/Cargo.toml` as a rust-analyzer linked project (or open that manifest
as the workspace root). Do not add the fixture packages to the product
workspace: their deliberately isolated feature and replacement graphs are the
behavior under test.

The final standalone package set is exactly:

```text
fixtures/features
fixtures/dependency/renamed_facade
fixtures/dependency/contract_mismatch
fixtures/dependency/contract_mismatch/incompatible_facade  # support package, not an independent owner
```

The repository has only `Cargo.lock` and `fixtures/Cargo.lock` outside ignored
build output. The fixture workspace is intentionally excluded from the main
workspace so each absence check resolves its own explicitly selected feature
set.

## Required feature matrix

Normal workspace commands prove successful composition for façade default,
no-default, all-features, JSON, records, CSV, NDJSON, CSV+NDJSON, multipart,
governor, OAuth2, dangerous raw response, dangerous custom retry, and supported
transport combinations. The isolated fixture proves absence and no-governor
fail-closed behavior. Enabled-feature success is never assigned to a separate
package merely to increase fixture count.

## Adding a proof

1. Name the invariant and its current primary owner.
2. Prefer a unit test for local policy, an integration test for a public
   semantic contract, and a UI case only for compiler-visible absence/privacy.
3. Use a standalone Cargo package only for rename, replacement, absence, or
   feature-unification behavior and add its graph reason to this document.
4. Import an actual canonical example rather than recreating its declaration.
5. Use scripted or loopback transport, finite explicit deadlines, and no
   external service.
6. Seed protected values whenever the change touches URL/header/auth/body/error
   projections and assert absence from every safe formatting channel.
