# Auth Completion Audit

Date: 2026-04-13
Scope: `concord_core`, `concord_macros`, `concord_examples` auth runtime and DSL wiring.

## 1. Current State (What Is Solid)

Implemented and working:
- Typed auth runtime shape exists (`AuthPart`, `AuthController`, `UseCredential`, `AuthChain`).
- Built-in credential material and usages exist (`AccessToken`, `ApiKey`, `BasicCredential`, `ClientCertificate`; `BearerAuth`, `HeaderAuth`, `QueryAuth`, `BasicAuth`, `CertificateAuth`).
- Slot single-flight for acquire/refresh exists (`CredentialSlot`).
- 401 challenge/retry flow exists for `UseCredential`.
- Internal auth HTTP executor exists (`AuthHttpExecutor` + `AuthHttpRequest`).
- Macro DSL supports credential declaration + usage binding, including `Custom<Provider>` and `Custom<Usage>`.
- Example tests pass for core and DSL custom provider/usage paths.

Evidence:
- `concord_core/src/auth/core.rs`
- `concord_core/src/auth/usage.rs`
- `concord_core/src/auth/credentials.rs`
- `concord_core/src/client.rs`
- `concord_examples/tests/auth_core.rs`
- `concord_examples/tests/auth_dsl.rs`

## 2. Critical Gaps Blocking "Complete" Auth

### P0-1. `AuthMode::UseAuth(...)` is declared but not implemented

- `AuthMode` exposes `UseAuth(AuthRequirementId)` (`concord_core/src/auth/http.rs:26-29`).
- Internal executor rejects any mode except `SkipAuth` (`concord_core/src/client.rs:872-876`).
- `AuthRequirementId` is not used anywhere else (dead surface).

Impact:
- Providers cannot request "internal request with another auth requirement".
- Cross-auth dependencies are forced into manual inline mutation only.

### P0-2. Recursion detection is missing

- `AuthErrorKind::RecursionDetected` exists (`concord_core/src/auth/errors.rs:27`) but is never emitted.
- No request reentry stack exists in `ClientAuthHttpExecutor`.

Impact:
- Recursive auth dependency chains can loop until runtime failure/timeout.

### P0-3. Failed credential state has no backoff semantics

- Slot state has `retry_after` (`concord_core/src/auth/credentials.rs:125-129`) but commit always sets `None` (`305-309`).

Impact:
- Repeated failures can aggressively hammer token/login endpoints.

### P0-4. Auth state update model is not clone-safe for secret-driven provider rebuilds

- Macro-generated secret setters rebuild auth state only on the current wrapper instance (`concord_macros/src/codegen.rs:870-889`, `908-918`).
- `ApiClient` stores `auth_state` by value (`concord_core/src/client.rs:62`) and `set_auth_state` is local mutation (`139-140`).

Impact:
- Two cloned clients sharing auth vars can diverge in provider slots after secret changes.
- Static providers built from secret snapshots can remain stale in clones not receiving `set_auth_state`.

### P0-5. Custom credential dependency tracking is incomplete

- Setter-triggered auth-state rebuild depends on `auth_credential_secret_names`.
- `Custom` credentials contribute no secret dependencies (`concord_macros/src/codegen.rs:312-336`, especially `332`).

Impact:
- Custom providers initialized from `secret.*` can silently keep stale captured values after setter changes.

### P0-6. Certificate auth is runtime-visible but transport-noop

- `CertificateAuth` writes `request.extensions.transport_auth` (`concord_core/src/auth/usage.rs:436-438`).
- Reqwest transport ignores extensions (`concord_core/src/transport.rs:228`).

Impact:
- mTLS/certificate auth appears supported at DSL/runtime level but has no transport effect.

## 3. High Priority Gaps (Strongly Recommended)

### P1-1. No global auth retry guard at request level

- `UseCredential` has per-step retry budget (`auth_retries`) (`concord_core/src/auth/usage.rs:107-109`, `217-222`).
- Outer request loop has no global auth retry cap.

Impact:
- Complex auth chains can retry more than intended.
- Custom controllers can accidentally create unbounded retry loops.

### P1-2. Internal auth policy is too narrow

- `AuthInternalPolicy` only has `timeout` (`concord_core/src/auth/http.rs:45-47`).

Impact:
- Cannot express explicit retry/rate-limit/inflight/cache behavior for internal auth calls.
- Hard to align with upcoming rate-limit/retry/caching roadmap.

### P1-3. Provenance is not wired from DSL layers

- `AuthAppliedPart.provenance` exists (`concord_core/src/auth/core.rs:304`) but generated usage does not set structured layer source.

Impact:
- Weak diagnostics for layered auth contracts (`client -> scope -> endpoint`).

### P1-4. Applied-part lookup can be ambiguous with duplicate usage IDs

- `UseCredential::on_response` finds by `(credential_id, usage_id)` and takes first match (`concord_core/src/auth/usage.rs:183-188`).

Impact:
- Multiple identical usage IDs for same credential can map to same applied part.

## 4. Medium Priority Gaps

### P2-1. DSL has no first-class built-in `CertificateAuth` syntax

- Runtime has `CertificateAuth`, parser only accepts `BearerAuth|HeaderAuth|QueryAuth|BasicAuth|Custom<T>` (`concord_macros/src/parse.rs:358-420`).

### P2-2. Legacy `AuthProvider` hook remains parallel to new typed auth runtime

- Still active in request pipeline (`concord_core/src/client.rs:372-378`, `699-706`).

Risk:
- Dual auth mutation surfaces complicate mental model and debugging.

## 5. Recommended Target Decisions

### Decision A: Make auth state shared and replaceable across clones

Use shared pointer indirection for auth state (for example shared `Arc` container with atomic/locked swap semantics), so `set_secret`-driven rebuild updates all clones.

Expected property:
- Any clone that changes a secret affecting providers updates provider graph globally for that logical client instance.

### Decision B: Define explicit internal auth resolution contract

Implement `UseAuth(AuthRequirementId)` via a resolver that maps requirement IDs to generated auth plans. Keep `SkipAuth` default.

### Decision C: Add recursion guard in internal auth executor

Track active internal auth requirements/credentials per task/request and fail fast with `RecursionDetected` on cycle.

### Decision D: Add credential failure backoff contract

Introduce retry hints in auth errors (or provider policy), persist into slot `retry_after`, enforce on immediate retries.

### Decision E: Make custom provider secret dependency safe-by-default

At minimum: any secret setter rebuilds auth state when custom credentials exist.

Better long-term:
- custom provider declares secret dependencies explicitly,
- codegen rebuilds only affected credentials.

## 6. PR Execution Plan

### PR-1: Internal auth correctness
- Implement `AuthMode::UseAuth(...)` in `ClientAuthHttpExecutor`.
- Add recursion guard and tests.
- Keep `SkipAuth` behavior unchanged.

### PR-2: Backoff + retry safety
- Add slot backoff wiring (`retry_after`).
- Add request-level global auth retry cap.
- Add tests for repeated auth failure behavior.

### PR-3: Clone-safe auth-state updates
- Introduce shared replaceable auth-state model in `ApiClient`.
- Update macro-generated secret setters to rebuild shared auth state.
- Add clone-consistency tests.

### PR-4: Custom dependency correctness
- Safe default rebuild strategy for custom credentials.
- Optional explicit dependency API.
- Add DSL tests proving custom provider uses updated secrets.

### PR-5: Certificate transport wiring
- Make transport honor `extensions.transport_auth` (or fail loudly if unsupported).
- Add integration test proving certificate selection path is applied.

### PR-6: Diagnostics hardening
- Wire auth provenance from client/scope/endpoint layers.
- Add stable step identity for applied-part matching.

## 7. Test Matrix Required for Completion

Must-have new tests:
- `internal_auth_useauth_applies_requirement`.
- `internal_auth_recursion_detected`.
- `credential_slot_failed_state_honors_retry_after`.
- `auth_retry_has_global_cap`.
- `secret_setter_rebuild_updates_all_client_clones`.
- `custom_provider_secret_update_rebuilds_state`.
- `certificate_auth_reaches_transport_extension`.
- `duplicate_usage_ids_do_not_misattribute_applied_part`.

Existing tests to keep:
- `concord_examples/tests/auth_core.rs`.
- `concord_examples/tests/auth_dsl.rs`.

## 8. Immediate Next Step

Start with PR-1 (`UseAuth` + recursion guard) because it unlocks real dependent auth flows and prevents pathological recursion before adding more auth features.
