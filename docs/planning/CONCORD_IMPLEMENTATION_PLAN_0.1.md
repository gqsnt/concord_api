# CONCORD_IMPLEMENTATION_PLAN_0.1
> **For agentic workers:** implement this plan task-by-task with fresh review checkpoints. Use test-driven development, small commits, and the conformance requirement IDs attached to every task.

**Version:** 0.1  
**Status:** Internal Implementation Candidate  
**Authority:** Delivery sequence and milestone exit criteria  
**Depends on:** All normative and architecture documents  
**Supersedes:** None

**Goal:** Build Concord 0.1 from a clean workspace as a typed generated Reqwest API client satisfying the complete normative specification set.

**Architecture:** Production execution uses concrete managed Reqwest clients. Generated endpoints prepare a public request head and body metadata, then use one core execution pipeline that runs the veto hook before expensive body materialization. Endpoint outputs and pagination state remain statically typed. Dynamic dispatch is limited to intentional extension points and private heterogeneous credential materialization.

**Tech stack:** Rust 2024, MSRV 1.85, Reqwest 0.13.4, Tokio, `bytes`, `http`, `url`, `syn`, `quote`, `proc-macro2`, `proc-macro-crate`, optional Serde/Serde JSON, optional Governor, Nextest, Trybuild, Proptest, Loom or Shuttle, Cargo Deny.

## Global constraints

- No previous workspace code is copied wholesale.
- No compatibility layer or deprecated API is introduced.
- `concord_core` and `concord_macros` use `#![forbid(unsafe_code)]`.
- Generated clients have no public transport or executor generic.
- No runtime `Any` response or pagination downcasting.
- No Concord general retry loop.
- No cache subsystem.
- No profile or generic policy subsystem.
- No public implementation adapter is added unless required by the Public API Specification.
- Every normative requirement has one planned row in `conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`; actual passing or waived evidence is recorded separately in `conformance/CONFORMANCE_EVIDENCE_0.1.csv`.
- A task is complete only when its tests pass under its required feature configurations.

---

## 1. Target workspace structure

```text
Cargo.toml
rust-toolchain.toml
rustfmt.toml
clippy.toml
deny.toml
justfile
.config/nextest.toml

concord_core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── client/
│   ├── request/
│   ├── execute/
│   ├── http/
│   ├── routing/
│   ├── headers/
│   ├── codec/
│   ├── body/
│   ├── response/
│   ├── auth/
│   ├── rate_limit/
│   ├── pagination/
│   ├── hooks/
│   ├── error/
│   ├── redaction/
│   ├── __private.rs
│   └── __development.rs
└── tests/

concord_macros/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── parse/
│   ├── ast/
│   ├── resolve/
│   ├── ir/
│   ├── validate/
│   ├── names/
│   ├── codegen/
│   └── diagnostics.rs
└── tests/trybuild/

concord_test_support/
├── Cargo.toml
└── src/lib.rs

concord_examples/
├── Cargo.toml
├── src/
└── tests/

perf/
├── Cargo.toml
├── src/
└── benches/
```

The `perf` package may remain outside the primary workspace if that avoids polluting normal dependency resolution.

---

## Milestone 0 — Workspace and specification gates

### Task 0.1: Create workspace manifests and toolchain policy

**Files:**

- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `rustfmt.toml`
- Create: `clippy.toml`
- Create: `.config/nextest.toml`
- Create: `deny.toml`
- Create: `justfile`
- Create: crate manifests listed above

**Produces:**

- Rust 2024 workspace;
- `rust-version = "1.85"` on public crates;
- exact Reqwest 0.13.4 workspace dependency;
- feature names from `FEAT-*`;
- Nextest is the canonical unit/integration runner with a minimum version, zero retries, default
  and CI profiles, finite timeouts, and bounded concurrency for Cargo-spawning tests. `cargo test`
  is reserved for doctests.

**Steps:**

- [ ] Create the workspace with empty `concord_core`, `concord_macros`, `concord_test_support`, and `concord_examples` crates.
- [ ] Add exact feature declarations to `concord_core/Cargo.toml`.
- [ ] Add `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` initially only where public scaffolding permits it; enable full missing-doc denial by Milestone 10.
- [ ] Add composable default/all/no-default check, Nextest, doctest, Rustdoc, and release recipes.
- [ ] Run `cargo check --workspace` under the pinned Rust 1.85 toolchain and fix all failures.
- [ ] Commit as `chore: bootstrap concord 0.1 workspace`.

**Verification:**

```bash
cargo fmt --all -- --check
cargo check --workspace
```

Expected: success with no warnings.

### Task 0.2: Establish specification coverage and evidence review

**Files:**

- Create: `conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`
- Create: `conformance/CONFORMANCE_EVIDENCE_0.1.csv`
- Create: phase conformance reports as implementation milestones complete

**Produces:**

A repeatable manual governance procedure that:

- reviews normative Markdown for stable requirement IDs, ownership, and dependency order;
- reviews the coverage plan for intentional mappings;
- reviews the evidence catalog for exact known requirements and valid results;
- distinguishes planned coverage from actual passing/waived evidence;
- requires exact actual evidence for release-final status;
- concludes with an audit of the maintained release command.

**Steps:**

- [ ] Review normative ownership and requirement IDs in dependency order.
- [ ] Review every coverage-plan mapping without treating it as evidence.
- [ ] Review actual evidence for exact artifacts, commands, results, and source commits.
- [ ] Audit the release recipe and record the review in the phase report.
- [ ] Commit as `docs: establish specification coverage and evidence governance`.

**Conformance:** `DOC-ID-*`, `CONF-TRACE-*`.

### Task 0.3: Prove macro/core path resolution and minimal vertical generation

**Files:**

- Create: `concord_macros/tests/trybuild/pass/minimal_generated_client.rs`
- Create: `concord_macros/tests/trybuild/pass/renamed_core_dependency.rs`
- Create: `concord_macros/tests/trybuild/pass/nested_module.rs`
- Create: `concord_macros/tests/trybuild/pass/multiple_api_invocations.rs`
- Create: `concord_core/src/__private.rs`

**Produces:**

- one generated fixed-base bodyless endpoint compiling through a minimal typed core stub;
- crate-path discovery that works when `concord_core` is renamed;
- unique generated aliases inside nested modules and repeated macro invocations.

**Steps:**

- [ ] Add failing trybuild fixtures for a renamed dependency and nested invocation.
- [ ] Resolve the caller-visible core path through `proc_macro_crate` or an equivalent validated mechanism.
- [ ] Generate a unique private alias rather than hardcoding `::concord_core`.
- [ ] Generate one minimal client, facade, endpoint, and direct-await type contract against a stub executor.
- [ ] Commit as `spike: prove generated core integration`.

**Conformance:** `COMPAT-MACRO-*`, `GEN-TYPE-*`, `CONF-COMPILE-*`.

### Task 0.4: Prove scoped facade and explicit endpoint parity

**Files:**

- Create: `concord_macros/tests/trybuild/pass/scoped_explicit_endpoint.rs`
- Create: `concord_macros/tests/generated_runtime/scoped_endpoint_parity.rs`

**Produces:**

- required outer scope values in explicit constructors;
- identical public request heads for facade-created and explicit scoped endpoints.

**Steps:**

- [ ] Define nested scopes with required, optional, and defaulted values.
- [ ] Compile the facade and explicit constructor forms.
- [ ] Compare method, logical URL, ordered query pairs, and public headers before authentication.
- [ ] Commit as `spike: prove scoped endpoint parity`.

**Conformance:** `GEN-EXPL-*`, `GEN-FACADE-*`.

### Task 0.5: Model credential rejection, replacement, cancellation, and stale completion

**Files:**

- Create: `concord_core/src/auth/model.rs`
- Create: `concord_core/tests/model/auth_slot.rs`

**Produces:**

A pure transition core proving:

- single-flight operation ownership;
- persistent rejection only for explicitly challenge-enabled unchanged static credentials;
- source-aware single challenge attribution;
- atomic source replacement;
- post-limiter credential finalization against clear, replacement, rejection, and expiry;
- cancellation liveness;
- no stale overwrite or rejected rollback.

**Steps:**

- [ ] Write a failing model scenario for a rejected static value followed by another prepare.
- [ ] Write replacement-versus-late-success, clear-versus-refresh, and preflight-versus-finalization scenarios.
- [ ] Model clear, slot replacement, rejection, and expiry completing during a limiter wait and assert no superseded material is finalized.
- [ ] Run the transition core under Loom or Shuttle over bounded interleavings.
- [ ] Keep actual provider futures outside the modeled state core.
- [ ] Commit as `spike(auth): model credential slot transitions`.

**Conformance:** `EXEC-AUTH-*`, `EXEC-RECOVER-*`, `CONF-AUTH-002`.

### Task 0.6: Probe Reqwest boundary assumptions

**Files:**

- Create: `concord_core/tests/reqwest_contract/redirect.rs`
- Create: `concord_core/tests/reqwest_contract/proxy.rs`
- Create: `concord_core/tests/reqwest_contract/retry.rs`
- Create: `concord_core/tests/reqwest_contract/timeout.rs`
- Create: `concord_core/tests/reqwest_contract/body_reuse.rs`

**Produces:**

Executable evidence for the selected Reqwest baseline concerning:

- redirects disabled;
- system proxies disabled;
- disabled-by-default and explicit Reqwest-default retry selection;
- native timeout behavior;
- request-body behavior under internal retry;
- unread terminal response drop, small complete terminal body capture, and oversized/abandoned terminal body connection behavior.

**Steps:**

- [ ] Use deterministic local servers and environment isolation.
- [ ] Assert the exact assumptions Concord depends on rather than Reqwest implementation details.
- [ ] Record any incompatibility as a specification or dependency-baseline change before runtime work continues.
- [ ] Commit as `test: pin reqwest boundary assumptions`.

**Conformance:** `SEC-HTTP-*`, `COMPAT-REQWEST-*`, `EXEC-NATIVE-*`.

### Task 0.7: Prove pagination identity and aggregate limits

**Files:**

- Create: `concord_core/tests/spikes/pagination_identity.rs`
- Create: `concord_core/tests/spikes/pagination_limits.rs`

**Produces:**

- canonical page-request equality;
- full equality after hash match;
- rejection of repeated one-, two-, and longer-state cycles;
- preflight page-ceiling rejection and item/identity fail-closed behavior.

**Steps:**

- [ ] Build canonical identities with reordered header insertion, repeated header values, default ports, and ordered query pairs.
- [ ] Prove semantically identical canonical requests compare equal and materially distinct requests do not.
- [ ] Prove the repeated request is rejected before hooks, credentials, limiter, or transport.
- [ ] Commit as `spike(pagination): prove identity and bounds`.

**Conformance:** `EXEC-PAGE-*`, `BOUND-PAGE-*`.

### Task 0.8: Prove prepared/generated executor parity

**Files:**

- Create: `concord_core/tests/spikes/prepared_generated_parity.rs`

**Produces:**

One prepared endpoint and one generated endpoint traversing the same typed executor and producing equivalent safe request heads and response behavior.

**Steps:**

- [ ] Implement only the minimum internal stubs needed for both endpoint forms.
- [ ] Assert neither form bypasses status handling, body limits, hooks, credentials, or limiter selection.
- [ ] Reject any design requiring a second executor before Milestone 1 begins.
- [ ] Commit as `spike: prove prepared and generated parity`.

**Conformance:** `API-PREP-*`, `PROD-CORE-003`.

### Milestone 0 exit criteria

- Workspace compiles on Rust 1.85.
- All feature names exist.
- Normative documents, coverage planning, and actual evidence pass recorded manual review.
- A minimal generated endpoint compiles with normal and renamed core dependencies.
- Scoped facade and explicit endpoints have equivalent public request heads.
- Credential slot invariants pass bounded model checking.
- Reqwest boundary assumptions are executable and passing.
- Pagination identity/ceiling spikes pass.
- Prepared and generated endpoints prove one executor design.
- No broad runtime API is stabilized before these spikes succeed.

---

## Milestone 1 — Error, redaction, HTTP configuration, and deterministic boundary

### Task 1.1: Implement safe error foundation

**Files:**

- Create: `concord_core/src/error/mod.rs`
- Create: `concord_core/src/error/category.rs`
- Create: `concord_core/src/error/context.rs`
- Create: `concord_core/src/error/transport.rs`
- Test: `concord_core/tests/public_api/error_api.rs`
- Test: `concord_core/tests/security/error_redaction.rs`

**Interfaces produced:**

- `ApiError`;
- `HttpResponseFailure`, `StatusBody`, `StatusBodyState`, and `StatusBodyDecodeError`;
- `ClientBuildError`;
- public category and subsystem-kind enums;
- safe endpoint/method/status/status-body accessors.

**Steps:**

- [ ] Write downstream-style compile tests for every required accessor.
- [ ] Write sentinel tests for `Display`, `Debug`, recursive source chains, and structural status-body wrappers.
- [ ] Write explicit positive-access tests proving complete terminal bytes are available only through `StatusBody::bytes()`/`into_bytes()`.
- [ ] Implement private error kinds and public wrappers.
- [ ] Implement raw Reqwest error reduction without retaining `reqwest::Error`.
- [ ] Run focused tests and Clippy.
- [ ] Commit as `feat(core): add structured safe errors`.

**Conformance:** `API-ERR-*`, `SEC-THREAT-001`, `SEC-URL-002`, `CONF-SEC-001`.

### Task 1.2: Implement central redaction

**Files:**

- Create: `concord_core/src/redaction/mod.rs`
- Create: `concord_core/src/redaction/names.rs`
- Create: `concord_core/src/redaction/headers.rs`
- Create: `concord_core/src/redaction/url.rs`
- Test: `concord_core/tests/security/redaction_matrix.rs`
- Property test: `concord_core/tests/property/safe_url.rs`

**Interfaces produced:**

- `SanitizedHeaders`;
- `SanitizedHeaderMap`;
- safe URL rendering;
- central sensitive-name classification.

**Steps:**

- [ ] Write table-driven tests for every mandatory sensitive-name category.
- [ ] Write repeated-query ordering tests.
- [ ] Write property test proving auth query values never occur in safe URLs.
- [ ] Implement borrowed and owned sanitized header views.
- [ ] Implement structural debug helpers.
- [ ] Commit as `feat(core): add central redaction boundary`.

**Conformance:** `SEC-NAME-*`, `SEC-HEAD-*`, `SEC-QUERY-*`.

### Task 1.3: Implement safe HTTP configuration and managed clients

**Files:**

- Create: `concord_core/src/http/config.rs`
- Create: `concord_core/src/http/proxy.rs`
- Create: `concord_core/src/http/application.rs`
- Create: `concord_core/src/http/provider.rs`
- Test: `concord_core/tests/public_api/http_config.rs`
- Test: `concord_core/tests/feature_boundary/tls.rs`
- Test: `concord_core/tests/security/proxy.rs`

**Interfaces produced:**

- `HttpConfigBuilder`;
- `RuntimeConfigBuilder` skeleton;
- `SafeProxy`;
- private `ApplicationHttp` and `ProviderHttpInner`.

**Steps:**

- [ ] Write compile tests proving no raw Reqwest builder/client accessor exists.
- [ ] Write proxy rejection tests for userinfo/path/query/fragment.
- [ ] Write no-default unauthenticated HTTP, fixed HTTPS failure, protected HTTP rejection, and provider non-HTTPS rejection tests.
- [ ] Implement plain config values and Reqwest client builders.
- [ ] Disable redirects, cookies, and system proxies.
- [ ] Implement application retry disabled by default, explicit Reqwest-default opt-in, and provider retry modes.
- [ ] Add dangerous custom retry builder application behind feature.
- [ ] Commit as `feat(core): add managed reqwest clients`.

**Conformance:** `API-HTTP-*`, `SEC-HTTP-*`, `FEAT-NODEFAULT-*`, `COMPAT-REQWEST-001`.

### Task 1.4: Implement private deterministic executor boundary

**Files:**

- Create: `concord_core/src/__development.rs`
- Create: `concord_test_support/src/lib.rs`
- Test: `concord_core/tests/feature_boundary/development.rs`
- Test: `concord_core/tests/runtime/deterministic_boundary.rs`

**Interfaces produced:**

- application/provider execution channels;
- scripted responses and errors;
- safe request capture;
- synchronization gates.

**Steps:**

- [ ] Write compile-fail fixture proving `__development` is absent without feature.
- [ ] Write test proving generated/core types have no executor generic.
- [ ] Implement final-boundary executor switch under `cfg`.
- [ ] Implement separate application/provider queues.
- [ ] Implement redacted fake-value expectation failures.
- [ ] Commit as `test(core): add deterministic reqwest boundary`.

**Conformance:** `FEAT-DEV-001`, `CONF-DET-*`.

### Milestone 1 exit criteria

- Safe errors and redaction pass sentinel tests.
- HTTP/no-default/TLS/proxy feature tests pass.
- Deterministic application/provider channels work.
- No endpoint abstraction exists yet.

---

## Milestone 2 — Typed request head and basic response pipeline

### Task 2.1: Implement routing and header ownership

**Files:**

- Create: `concord_core/src/routing/host.rs`
- Create: `concord_core/src/routing/path.rs`
- Create: `concord_core/src/routing/query.rs`
- Create: `concord_core/src/headers/ownership.rs`
- Test: `concord_core/tests/runtime/routing.rs`
- Property: `concord_core/tests/property/path_segments.rs`

**Produces:**

- validated host labels;
- one-segment path encoding;
- ordered query pairs;
- public header ownership validation.

**Conformance:** `DSL-HOST-*`, `DSL-PATH-*`, `DSL-QUERY-*`, `DSL-HEAD-*`, `SEC-CLOSED-001`.

### Task 2.2: Implement `ClientCore`, `Request<E>`, and sealed `EndpointSpec`

**Files:**

- Create: `concord_core/src/client/core.rs`
- Create: `concord_core/src/request/mod.rs`
- Create: `concord_core/src/request/endpoint.rs`
- Create: `concord_core/src/request/head.rs`
- Create: `concord_core/src/request/options.rs`
- Create: `concord_core/src/__private.rs`
- Test: `concord_core/tests/downstream/minimal_endpoint.rs`

**Interfaces produced:**

- private/generated `EndpointSpec`;
- typed `Request<E>`;
- `PreparedHead`;
- `HeadBuilder`;
- generated-contract marker.

**Steps:**

- [ ] Write a hand-authored generated-style endpoint fixture.
- [ ] Prove request creation has no deterministic executor events.
- [ ] Implement head/body split.
- [ ] Implement logical and safe URL retention.
- [ ] Implement compile-time generated marker assertion.
- [ ] Commit as `feat(core): add typed request preparation`.

**Conformance:** `PROD-CORE-*`, `EXEC-CREATE-*`, `EXEC-HEAD-*`, `COMPAT-GEN-*`.

### Task 2.3: Implement basic execution and response handlers

**Files:**

- Create: `concord_core/src/execute/operation.rs`
- Create: `concord_core/src/execute/attempt.rs`
- Create: `concord_core/src/execute/status.rs`
- Create: `concord_core/src/response/bytes.rs`
- Create: `concord_core/src/response/no_content.rs`
- Test: `concord_core/tests/runtime/basic_execution.rs`
- Test: `concord_core/tests/runtime/status.rs`

**Produces:**

- one Reqwest execution per Concord attempt;
- bytes response;
- no-content response;
- status errors;
- direct `IntoFuture`.

**Steps:**

- [ ] Write failing deterministic GET bytes test.
- [ ] Implement empty request body path.
- [ ] Implement response-head observation placeholders.
- [ ] Implement status classification and bodyless status errors.
- [ ] Implement no-content body skip.
- [ ] Implement completion dispatch skeleton.
- [ ] Commit as `feat(core): execute typed basic endpoints`.

**Conformance:** `EXEC-NATIVE-*`, `EXEC-STATUS-*`, `EXEC-NOCONTENT-*`.

### Task 2.4: Deliver one real generated endpoint vertical slice

**Files:**

- Create: `concord_macros/src/minimal.rs`
- Create: `concord_macros/tests/generated_runtime/minimal_real_executor.rs`
- Modify: `concord_macros/src/lib.rs`
- Modify: `concord_core/src/__private.rs`

**Produces:**

A minimal DSL subset covering one client, fixed base URL, one scope, one bodyless `GET`, text/bytes/no-content response selection, and direct await through the real managed Reqwest executor.

**Steps:**

- [ ] Reuse the crate-path and naming proof from Milestone 0 rather than introducing temporary public paths.
- [ ] Generate one real `ClientCore`, facade, endpoint value, pending request, and sealed endpoint implementation.
- [ ] Execute against the deterministic Reqwest boundary and verify safe URL, status, limit, hook, and completion behavior.
- [ ] Compile the same fixture with the core dependency renamed and the macro invocation nested.
- [ ] Keep this slice intentionally narrow; later macro milestones extend the same AST/IR/codegen path rather than replacing it.
- [ ] Commit as `feat: deliver minimal generated endpoint vertical slice`.

**Conformance:** `PROD-CORE-*`, `GEN-TYPE-*`, `GEN-FACADE-*`, `GEN-END-*`, `EXEC-ORDER-001`, `COMPAT-MACRO-*`.

### Milestone 2 exit criteria

- Hand-authored and minimal generated typed endpoints execute through one concrete managed Reqwest path.
- The generated vertical slice works with normal and renamed core dependencies.
- Bytes and no-content work.
- No runtime `Any`, temporary generated executor, or middleware chain exists.
- Terminal status classification works with capture disabled; Milestone 3 adds finite complete-only status-body capture before the response API is considered stable.

---

## Milestone 3 — Hooks, debug, codecs, and buffered responses

### Task 3.1: Implement hooks and completion model

**Files:**

- Create: `concord_core/src/hooks/mod.rs`
- Create: `concord_core/src/hooks/events.rs`
- Create: `concord_core/src/hooks/debug.rs`
- Test: `concord_core/tests/runtime/hooks.rs`
- Test: `concord_core/tests/security/hook_redaction.rs`

**Produces:** `RuntimeHooks`, opaque operation identifiers, request/response/completion contexts, parent pagination correlation fields, debug levels, and one request/page completion event.

**Steps:**

- [ ] Generate one opaque `OperationId` when a request/page future first begins polling.
- [ ] Carry the same identifier through before-send, response-head, recovery, and completion events.
- [ ] Record monotonic elapsed duration and final application status when one existed.
- [ ] Add redaction tests proving operation metadata contains no body, credential, native URL, cursor, or limiter-key values.
- [ ] Add concurrent same-endpoint tests proving events can be correlated without URL uniqueness.
- [ ] Commit as `feat(core): add correlated lifecycle hooks`.

**Conformance:** `API-HOOK-*`, `EXEC-OPID-*`, `EXEC-HOOK-*`, `EXEC-RESPHEAD-*`, `EXEC-COMPLETE-*`.

### Task 3.2: Implement codec contracts and built-in text

**Files:**

- Create: `concord_core/src/codec/mod.rs`
- Create: `concord_core/src/codec/text.rs`
- Test: `concord_core/tests/downstream/custom_codec.rs`
- Test: `concord_core/tests/security/codec_errors.rs`

**Produces:** `MediaType`, `EncodeBody`, `DecodeBody`, contexts, safe codec errors.

### Task 3.3: Implement buffered body/response limits

**Files:**

- Create: `concord_core/src/body/limit.rs`
- Create: `concord_core/src/response/buffered.rs`
- Create: `concord_core/src/response/status_body.rs`
- Property: `concord_core/tests/property/body_limit.rs`
- Test: `concord_core/tests/runtime/response_limits.rs`
- Test: `concord_core/tests/runtime/status_body.rs`
- Test: `concord_core/tests/security/status_body_redaction.rs`

**Produces:** known-length preflight, first-excess-frame withholding, finite complete-only terminal status-body capture, structural capture states, and explicit codec decoding.

**Steps:**

- [ ] Implement the independent positive finite status-body limit and disabled-capture state.
- [ ] Preserve a small complete terminal body and expose it only through `HttpResponseFailure`/`StatusBody`.
- [ ] Reject known oversized terminal bodies without polling and record `OmittedKnownTooLarge`.
- [ ] On unknown-length overflow discard every collected byte, expose no prefix, and record `OmittedLimitExceeded`.
- [ ] Preserve the known HTTP status on body read failure or operation-deadline expiry and record the structural unavailable state.
- [ ] Count bytes after enabled decompression.
- [ ] Implement owned extraction and `decode_body<C>()` only for complete retained bodies.
- [ ] Prove `Display`, `Debug`, source chains, hooks, limiter observation, and completion events remain body-free.
- [ ] Probe recoverable challenge bodies remaining unread and terminal challenge bodies using bounded capture.
- [ ] Commit as `feat(core): add bounded terminal status bodies`.

**Conformance:** `API-ERR-004` through `API-ERR-008`, `SEC-RESP-*`, `BOUND-BODY-009`, `BOUND-BODY-010`, `EXEC-STATUS-*`.

### Task 3.4: Implement JSON feature

**Files:**

- Create: `concord_core/src/codec/json.rs`
- Test: `concord_core/tests/feature_boundary/json.rs`
- Test: `concord_core/tests/runtime/json.rs`

**Conformance:** `FEAT-JSON-*`, media matching and body-limit requirements.

### Task 3.5: Implement `DecodedResponse<T>` and raw-response feature

**Files:**

- Create: `concord_core/src/response/decoded.rs`
- Create: `concord_core/src/response/raw.rs`
- Test: `concord_core/tests/public_api/responses.rs`
- Test: `concord_core/tests/feature_boundary/raw_response.rs`

### Milestone 3 exit criteria

- Custom codecs work downstream.
- Buffered limits and decode ordering are proven.
- Hooks/debug/completion behavior is correlated and stable.
- Terminal non-success responses expose only finite complete bodies through the explicit opaque status-body API.
- Raw response is feature-gated and bounded.

---

## Milestone 4 — Request streams, response streams, factories, and multipart

### Task 4.1: Implement managed request streams

**Files:**

- Create: `concord_core/src/body/stream.rs`
- Modify: `concord_core/src/body/mod.rs`
- Modify: `concord_core/src/body/prepared.rs`
- Test: `concord_core/tests/runtime/request_stream.rs`
- Test: `concord_core/tests/security/request_stream_redaction.rs`
- Property: `concord_core/tests/property/request_stream_limit.rs`

**Interfaces consumed:**

- `BodyError`, `BodyErrorKind` from Task 1.1;
- `RuntimeLimits` and request-body limit configuration from Task 3.3;
- private `BodySpec` and `PreparedBody` from Milestone 2;
- deterministic executor body-polling gates from Task 1.4.

**Interfaces produced:**

- public `StreamBody` constructors and exact-length metadata;
- private managed native request-body adapter;
- safe conversion of producer, exact-length, and aggregate-limit failures.

**Steps:**

- [ ] Add downstream compile tests for `StreamBody::from_stream`, `from_reader`, and `from_file` under the signatures in `API-BODY-*`.
- [ ] Add deterministic tests proving the stream is not polled during request construction, head validation, body metadata validation, or hook setup.
- [ ] Add exact-length underflow and overflow tests, including a frame that crosses both exact length and aggregate request limit.
- [ ] Add a property test generating arbitrary frame boundaries and proving the first excess byte is never delivered through the managed body boundary.
- [ ] Implement the managed `http_body::Body<Data = Bytes>` adapter with checked cumulative byte accounting.
- [ ] Convert producer errors into `BodyErrorKind::Producer` without retaining user text or body fragments.
- [ ] Verify cancellation drops the upstream stream and performs no later polling.
- [ ] Run `cargo nextest run -p concord_core -E 'binary(request_stream)'` and the focused security/property tests.
- [ ] Commit as `feat(core): add bounded request streams`.

**Conformance:** `API-BODY-*`, `EXEC-BODY-*`, `EXEC-NATIVE-*`, `BOUND-BODY-*`, `SEC-BODY-*`.

### Task 4.2: Implement body factories and public `RequestBody`

**Files:**

- Create: `concord_core/src/body/factory.rs`
- Modify: `concord_core/src/body/prepared.rs`
- Modify: `concord_core/src/body/mod.rs`
- Test: `concord_core/tests/downstream/body_factory.rs`
- Test: `concord_core/tests/runtime/body_factory.rs`

**Interfaces consumed:**

- public `BodyFactory`, `RequestBody`, and `BodyError` signatures;
- private prepared-body replayability model;
- deterministic executor request counts.

**Interfaces produced:**

- supported advanced request-body constructors;
- reconstructible attempt bodies for authentication recovery;
- body-kind and known-length metadata for hooks/debugging.

**Steps:**

- [ ] Add a downstream fixture implementing `BodyFactory` without importing `__private`.
- [ ] Add tests proving a factory is not called by pending-request creation, public-head preparation, replayability inspection, or Reqwest-internal retries.
- [ ] Add a test proving one call for recovery index zero and one new call for recovery index one.
- [ ] Add tests for factory error sanitization and incorrect exact-length metadata.
- [ ] Implement the public `RequestBody` constructors as safe wrappers around private body variants.
- [ ] Implement factory attempt materialization with no lock held during the synchronous factory call.
- [ ] Run downstream and deterministic runtime tests.
- [ ] Commit as `feat(core): add reconstructible request bodies`.

**Conformance:** `API-BODY-*`, `EXEC-BODY-*`, `EXEC-NATIVE-*`, `EXEC-RECOVER-*`.

### Task 4.3: Implement bounded response streams

**Files:**

- Create: `concord_core/src/response/stream.rs`
- Modify: `concord_core/src/response/mod.rs`
- Modify: `concord_core/src/request/terminals.rs`
- Test: `concord_core/tests/public_api/stream_response.rs`
- Test: `concord_core/tests/runtime/response_stream.rs`
- Test: `concord_core/tests/security/stream_response.rs`

**Interfaces consumed:**

- `MediaType` and response-head validation;
- `ExecutionMeta` and safe response metadata;
- streaming response limit from RuntimeLimits;
- response-head hook and completion semantics.

**Interfaces produced:**

- public `StreamResponse<M>`;
- private bounded Reqwest byte stream;
- streaming response-handler marker used by generated and prepared endpoints.

**Steps:**

- [ ] Add compile tests for stream metadata accessors and `Stream<Item = Result<Bytes, ApiError>>`.
- [ ] Add deterministic tests for known `Content-Length` rejection before stream return.
- [ ] Add media tests for type/subtype equality, required parameter matching, and allowed additional server parameters.
- [ ] Add chunk-boundary tests proving the first excess chunk is not yielded.
- [ ] Add cancellation tests proving dropping the stream stops further body polling.
- [ ] Implement completion-at-stream-creation and verify later chunk errors create no second completion event.
- [ ] Run stream public API, runtime, and redaction tests.
- [ ] Commit as `feat(core): add bounded response streams`.

**Conformance:** `API-RESP-*`, `EXEC-STREAM-*`, `BOUND-BODY-*`, `SEC-BODY-*`.

### Task 4.4: Implement multipart request support

**Files:**

- Create: `concord_core/src/body/multipart.rs`
- Modify: `concord_core/src/body/mod.rs`
- Modify: `concord_core/Cargo.toml`
- Test: `concord_core/tests/feature_boundary/multipart.rs`
- Test: `concord_core/tests/runtime/multipart.rs`
- Test: `concord_core/tests/security/multipart.rs`

**Interfaces consumed:**

- Reqwest multipart under the `multipart` feature;
- `StreamBody`, reusable bytes, and body factories;
- aggregate request-body limiting.

**Interfaces produced:**

- `MultipartBody`;
- safe text, byte, file, and stream part constructors;
- `MultipartFactory` and private replayability classification.

**Steps:**

- [ ] Add compile-fail fixture proving multipart APIs and DSL markers are absent without the feature.
- [ ] Add tests for reusable all-buffered recipes, one-shot stream recipes, and reconstructible multipart factories.
- [ ] Add deterministic tests proving Reqwest owns boundary generation and Concord never manually serializes multipart syntax.
- [ ] Add aggregate-limit tests where payload bytes fit but payload plus multipart framing exceeds the limit.
- [ ] Add redaction tests for text values, byte values, filenames, and producer errors.
- [ ] Implement part metadata validation and conversion to Reqwest multipart forms at attempt materialization.
- [ ] Run default, multipart-only, and all-feature tests.
- [ ] Commit as `feat(core): add multipart request bodies`.

**Conformance:** `API-BODY-*`, `EXEC-BODY-*`, `BOUND-BODY-*`, `FEAT-MULTIPART-*`.

### Milestone 4 exit criteria

- Direct streams remain one-shot and lazy.
- Reconstructible factories run once per Concord attempt and never per hidden retry.
- Response streams are bounded before excess delivery.
- Multipart is request-only, Reqwest-serialized, aggregate-bounded, and feature-gated.
- No body implementation exposes payload bytes through ordinary diagnostics.

---

## Milestone 5 — DSL compiler and generated API foundation

### Task 5.1: Implement lexical parsing and source AST

**Files:**

- Create: `concord_macros/src/parse/mod.rs`
- Create: `concord_macros/src/parse/client.rs`
- Create: `concord_macros/src/parse/value.rs`
- Create: `concord_macros/src/parse/scope.rs`
- Create: `concord_macros/src/parse/endpoint.rs`
- Create: `concord_macros/src/parse/expression.rs`
- Create: `concord_macros/src/ast/*`
- Test: `concord_macros/src/parse/tests/*`

**Interfaces produced:**

- span-preserving AST for base, values, route composition, scopes, endpoints, body/response markers, timeout, headers, and query;
- restricted public expression AST.

**Steps:**

- [ ] Write parser tests for every grammar production currently required before credentials, rate limits, and pagination.
- [ ] Write parser failures for configuration after children, duplicate base, missing parentheses, clause order, missing semicolons, and `fmt` punctuation.
- [ ] Implement dedicated parsers with no semantic lookup or generated-name logic.
- [ ] Add scope-depth and format-depth guards at token entry.
- [ ] Verify diagnostics identify the smallest useful source span.
- [ ] Commit as `feat(macros): parse concord client declarations`.

**Conformance:** `DSL-LEX-*`, `DSL-TOP-*`, `DSL-ORDER-*`, `DSL-VAL-*`, `DSL-SCOPE-*`, `DSL-END-*`.

### Task 5.2: Implement symbol resolution, validation, and normalized IR

**Files:**

- Create: `concord_macros/src/resolve/*`
- Create: `concord_macros/src/ir/*`
- Create: `concord_macros/src/validate/*`
- Create: `concord_macros/src/names.rs`
- Test: `concord_macros/src/resolve/tests/*`
- Test: `concord_macros/src/validate/tests/*`

**Interfaces consumed:**

- parsed AST and source spans.

**Interfaces produced:**

- stable semantic ids;
- fully normalized client/scope/endpoint IR;
- generated-name map and collision diagnostics.

**Steps:**

- [ ] Write semantic tests for client/scope/endpoint namespaces and ambiguous public references.
- [ ] Write normalization tests proving host/path/query/header/timeout composition is fully resolved before code generation.
- [ ] Write tests for DNS-label and one-segment path validation of static declarations.
- [ ] Write generated-name collision tests covering aliases, Rust keywords, clear/reset/with helper names, and explicit endpoint modules.
- [ ] Implement symbol-table construction independently from generated Rust names.
- [ ] Implement deterministic outer-to-inner normalization.
- [ ] Assert normalized IR contains no append/replace/off operations or unresolved ids.
- [ ] Commit as `feat(macros): resolve and normalize concord declarations`.

**Conformance:** `DSL-NAME-*`, `DSL-NORM-*`, `DSL-NAME-*`, `CONF-SEMA-001`.

### Task 5.3: Implement generated client builder and fixed client snapshots

**Files:**

- Create: `concord_macros/src/codegen/client.rs`
- Create: `concord_macros/src/codegen/builder.rs`
- Create: `concord_macros/src/codegen/vars.rs`
- Test: `concord_macros/tests/trybuild/pass/client_builder.rs`
- Test: `concord_macros/tests/trybuild/pass/fixed_client_snapshot.rs`
- Test: `concord_macros/tests/trybuild/fail/missing_required_builder_value.rs`
- Test: `concord_macros/tests/generated_runtime/builder_closures.rs`

**Interfaces consumed:**

- `ClientCoreBuilder`, `HttpConfigBuilder`, and `RuntimeConfigBuilder`;
- exact generated names from `GEN-TYPE-*`, `GEN-BUILD-*`, and `GEN-CLONE-*`.

**Interfaces produced:**

- generated client;
- canonical builder;
- fallible convenience constructor;
- fixed immutable client-value storage;
- cheap same-snapshot clones.

**Steps:**

- [ ] Add downstream type checks for required, optional, and defaulted builder methods.
- [ ] Prove an overridden default expression is not evaluated and selected defaults follow `DSL-VAL-*`.
- [ ] Generate one immutable client snapshot without post-build `with_*`, `without_*`, or `reset_*` client-value methods.
- [ ] Make `.http(F)` and `.runtime(F)` invoke `F` immediately and store the resulting configuration.
- [ ] Prove a duplicate call records a build error and does not invoke the second closure.
- [ ] Prove clones share core and credential handles only because they retain the same client-value snapshot.
- [ ] Commit as `feat(macros): generate fixed client construction`.

**Conformance:** `GEN-TYPE-*`, `GEN-BUILD-*`, `GEN-DEFAULT-*`, `GEN-CLONE-*`.

### Task 5.4: Implement facades, endpoint values, setters, and typed endpoint integration

**Files:**

- Create: `concord_macros/src/codegen/facade.rs`
- Create: `concord_macros/src/codegen/endpoint.rs`
- Create: `concord_macros/src/codegen/request.rs`
- Modify: `concord_core/src/__private.rs`
- Test: `concord_macros/tests/trybuild/pass/facades.rs`
- Test: `concord_macros/tests/trybuild/pass/endpoint_setters.rs`
- Test: `concord_macros/tests/generated_runtime/basic_execution.rs`

**Interfaces produced:**

- owned facade snapshots;
- endpoint value structs;
- inherent request setters;
- sealed typed `EndpointSpec` implementations;
- `client.request(endpoint)` for explicit generated endpoint values.

**Steps:**

- [ ] Add compile tests for required scope and endpoint argument ordering.
- [ ] Add tests for optional clear and default reset semantics at facade and request levels.
- [ ] Generate endpoint values containing only public snapshots and source-specific credential references needed by the endpoint.
- [ ] Generate borrowed `prepare_head_and_body_metadata` and consuming `into_body` so body ownership transfers only after public validation and before post-hook materialization.
- [ ] Verify endpoint method calls create pending requests without invoking runtime behavior.
- [ ] Commit as `feat(macros): generate typed facades and endpoint requests`.

**Conformance:** `GEN-FACADE-*`, `GEN-END-*`, `GEN-END-*`, `EXEC-CREATE-*`.

### Task 5.5: Implement body/response markers and request terminals

**Files:**

- Create: `concord_macros/src/codegen/io.rs`
- Create: `concord_macros/src/codegen/terminals.rs`
- Test: `concord_macros/tests/trybuild/pass/endpoint_io.rs`
- Test: `concord_macros/tests/trybuild/fail/unsupported_response_terminal.rs`
- Test: `concord_macros/tests/generated_runtime/io.rs`

**Steps:**

- [ ] Generate typed body specs for text, custom codecs, streams, and feature-gated multipart.
- [ ] Generate typed response handlers for text, custom codecs, bytes, no-content, and streams.
- [ ] Add JSON generation under the `json` feature.
- [ ] Verify direct await returns semantic output and `.response()` exists only for buffered decoded/bytes/no-content outputs.
- [ ] Verify streaming direct await returns `StreamResponse<M>` and has no redundant metadata terminal.
- [ ] Add raw-response method under its dangerous feature.
- [ ] Commit as `feat(macros): generate typed io and terminals`.

**Conformance:** `GEN-TERM-*`, `GEN-TERM-*`, `FEAT-JSON-*`, `FEAT-DANGER-*`.

### Task 5.6: Implement explicit endpoint modules, Rustdoc, and generated contract marker

**Files:**

- Create: `concord_macros/src/codegen/modules.rs`
- Create: `concord_macros/src/codegen/docs.rs`
- Modify: `concord_core/src/__private.rs`
- Test: `concord_macros/tests/trybuild/pass/explicit_endpoint.rs`
- Test: `concord_macros/tests/trybuild/pass/scoped_explicit_endpoint.rs`
- Test: `concord_macros/tests/trybuild/fail/generated_contract_mismatch.rs`
- Test: `concord_macros/tests/rustdoc.rs`

**Steps:**

- [ ] Generate stable endpoint module paths and scope-complete constructors from the naming service.
- [ ] Include required outer scope values outermost-to-innermost before required endpoint arguments.
- [ ] Generate optional/default scope controls needed to match facade snapshots.
- [ ] Generate Rustdoc containing safe endpoint facts and feature requirements without default-expression values or secret details.
- [ ] Add one private semantic generated-contract marker and mismatch fixture.
- [ ] Verify generated modules expose no storage, runtime plan, auth plan, or pagination adapter types.
- [ ] Commit as `feat(macros): generate explicit endpoints and contract marker`.

**Conformance:** `GEN-EXPL-*`, `GEN-DOC-*`, `COMPAT-GEN-*`.

### Milestone 5 exit criteria

- Minimal, routing, I/O, streaming, multipart, and explicit-endpoint fixtures compile from the DSL.
- Generated clients use immutable snapshots and typed endpoint outputs.
- No generated API requires a private import.
- No profile, generic policy object, runtime decoder adapter, or public transport abstraction is generated.

---

## Milestone 6 — Authentication

### Task 6.1: Implement secret and credential material types

**Files:**

- Create: `concord_core/src/auth/material.rs`
- Create: `concord_core/src/auth/error.rs`
- Modify: `concord_core/src/auth/mod.rs`
- Test: `concord_core/tests/public_api/credential_material.rs`
- Test: `concord_core/tests/security/credential_material.rs`

**Interfaces produced:**

- `SecretString`;
- `CredentialMaterial`;
- `SecretCredential`;
- `AccessToken`, `ApiKey`, `BasicCredential`;
- `AuthError`, `AuthErrorKind`, and recovery enums.

**Steps:**

- [ ] Add `Display`/`Debug` sentinel tests for every built-in material field, including Basic username.
- [ ] Implement expiration metadata without exposing material.
- [ ] Implement protected secret exposure only through the credential materialization boundary.
- [ ] Add checked expiration arithmetic tests.
- [ ] Commit as `feat(auth): add credential material types`.

**Conformance:** `API-MAT-*`, `API-PROV-*`, `SEC-SECRET-*`, `BOUND-AUTH-*`.

### Task 6.2: Implement provider contracts and isolated provider HTTP

**Files:**

- Create: `concord_core/src/auth/provider.rs`
- Create: `concord_core/src/auth/provider_http.rs`
- Test: `concord_core/tests/downstream/custom_provider.rs`
- Test: `concord_core/tests/runtime/provider_http.rs`
- Test: `concord_core/tests/security/provider_http.rs`

**Interfaces produced:**

- `CredentialProvider`;
- `CredentialRequest`;
- `ProviderHttp`, request/body/response types;
- provider request and response limit enforcement.

**Steps:**

- [ ] Add downstream provider fixture using only advanced APIs.
- [ ] Add tests proving provider HTTP invokes no application hook, limiter, debug event, pagination, codec, or dangerous application retry.
- [ ] Add provider body-factory invocation and independent request/response limit tests.
- [ ] Add safe status-return tests proving provider logic classifies non-success responses itself.
- [ ] Implement provider transport with no nested Concord credential authentication in 0.1.
- [ ] Commit as `feat(auth): add isolated credential provider http`.

**Conformance:** `API-PROV-*`, `API-PHTTP-*`, `EXEC-PHTTP-*`, `SEC-PROV-*`.

### Task 6.3: Implement replaceable slot references and credential state machine

**Files:**

- Create: `concord_core/src/auth/handle.rs`
- Create: `concord_core/src/auth/slot.rs`
- Create: `concord_core/src/auth/cohort.rs`
- Test: `concord_core/tests/concurrency/credential_slot.rs`
- Property: `concord_core/tests/property/credential_sequences.rs`

**Interfaces consumed:**

- `CONCORD_AUTH_STATE_MACHINE_0.1` in full.

**Interfaces produced:**

- public read-only `CredentialRef<M>`;
- public manual `CredentialHandle<M>`;
- private slot reference, slot identity, version token, operation cohort, and cancellation guard.

**Steps:**

- [ ] Write deterministic gates for acquisition leader, refresh leader, waiters, cancellation, manual set, clear, and slot replacement.
- [ ] Prove all current waiters receive one operation's same success or failure.
- [ ] Prove no slot lock is held across provider execution.
- [ ] Prove cancellation wakes waiters and cannot restore an invalidated previous value.
- [ ] Prove stale acquisition/refresh/rejection cannot affect newer state or replacement slots.
- [ ] Implement checked version and operation-id increment with explicit internal failure on overflow.
- [ ] Implement atomic static compound-input replacement through slot-reference swap.
- [ ] Commit as `feat(auth): add coordinated credential slots`.

**Conformance:** `API-HANDLE-*`, `EXEC-AUTH-*`, `EXEC-RECOVER-*`, `BOUND-AUTH-*`.

### Task 6.4: Implement private heterogeneous authentication and placement

**Files:**

- Create: `concord_core/src/auth/erased.rs`
- Create: `concord_core/src/auth/requirement.rs`
- Create: `concord_core/src/auth/materialize.rs`
- Test: `concord_core/tests/runtime/auth_placement.rs`
- Test: `concord_core/tests/security/auth_placement.rs`

**Interfaces produced:**

- private erased credential preparation boundary;
- request-local preparation cache;
- bearer, Basic, header, and query materialization;
- exact applied tokens.

**Steps:**

- [ ] Add public collision tests proving no provider call before header/query conflict failure.
- [ ] Add tests for one preparation when the same credential is placed more than once.
- [ ] Add tests proving hook/debug/limiter logical metadata excludes materialized auth.
- [ ] Implement private erasure without exposing material, slots, versions, or leases publicly.
- [ ] Commit as `feat(auth): add protected authentication placement`.

**Conformance:** `API-AUTH-*`, `EXEC-AUTH-*`, `SEC-URL-*`, `SEC-HEAD-*`.

### Task 6.5: Implement built-in static and OAuth2 providers

**Files:**

- Create: `concord_core/src/auth/static_provider.rs`
- Create: `concord_core/src/auth/oauth2.rs`
- Test: `concord_core/tests/runtime/static_auth.rs`
- Test: `concord_core/tests/runtime/oauth2.rs`
- Test: `concord_core/tests/security/oauth2.rs`
- Feature test: `concord_core/tests/feature_boundary/oauth2_json.rs`

**Steps:**

- [ ] Implement static API-key, bearer, and Basic providers with `RecoveryMode::None`.
- [ ] Implement exact OAuth2 client-credentials form and Basic request shape from `DSL-CRED-*`.
- [ ] Validate token endpoint HTTPS, access token, optional bearer token type, checked integer expiration, and scope.
- [ ] Reacquire on refresh; do not implement refresh-token grant.
- [ ] Bound and redact all provider responses and errors.
- [ ] Verify OAuth support is absent without `json`.
- [ ] Commit as `feat(auth): add built-in credential providers`.

**Conformance:** `DSL-CRED-*`, `API-PROV-*`, `EXEC-PHTTP-*`, `SEC-PROV-*`, `FEAT-JSON-*`.

### Task 6.6: Implement challenge recovery

**Files:**

- Create: `concord_core/src/auth/recovery.rs`
- Modify: `concord_core/src/execute/operation.rs`
- Test: `concord_core/tests/runtime/auth_recovery.rs`
- Test: `concord_core/tests/concurrency/auth_recovery_races.rs`

**Steps:**

- [ ] Add source-aware default tests for static, endpoint-backed, refreshable, custom, and multi-credential plans.
- [ ] Prove one ambiguous `401` mutates only the single challenge-enabled credential and leaves other applied credentials valid.
- [ ] Add one-shot body rejection tests proving no second send.
- [ ] Add recovery body factory tests and second-hook-veto tests.
- [ ] Add proactive-refresh/challenge race, newer-ready-value reuse, and recovery post-limiter finalization tests.
- [ ] Add terminal second-challenge tests proving no third attempt and bounded terminal challenge-body capture.
- [ ] Implement exact-version `recover_exact` integration and complete-plan validation.
- [ ] Commit as `feat(auth): add bounded authentication recovery`.

**Conformance:** `EXEC-RECOVER-*`, `EXEC-COUNT-*`, `BOUND-AUTH-003`.

### Task 6.7: Extend DSL and generated API for credentials and auth

**Files:**

- Modify: `concord_macros/src/parse/credentials.rs`
- Modify: `concord_macros/src/resolve/credentials.rs`
- Modify: `concord_macros/src/ir/auth.rs`
- Create: `concord_macros/src/codegen/credentials.rs`
- Create: `concord_macros/src/codegen/auth.rs`
- Test: `concord_macros/tests/trybuild/pass/auth.rs`
- Test: `concord_macros/tests/trybuild/fail/auth/*`
- Test: `concord_macros/tests/generated_runtime/auth.rs`

**Steps:**

- [ ] Parse and resolve every credential source and request placement.
- [ ] Validate placement compatibility, challenge policies, composition, and endpoint-backed cycles.
- [ ] Generate builder secret/provider inputs and source-specific credential accessors.
- [ ] Generate endpoint-backed acquisition terminal and compile-check exact output material.
- [ ] Generate only read-only credential refs into prepared endpoint auth attachments.
- [ ] Commit as `feat(macros): generate credentials and authentication`.

**Conformance:** `DSL-CRED-*`, `DSL-AUTH-*`, `GEN-CRED-*`, `GEN-AUTHFLOW-*`.

### Milestone 6 exit criteria

- All credential source categories compile and execute.
- Credential concurrency, cancellation, exact rejection, slot replacement, and race property tests pass.
- Static credentials are not immediately resent.
- Endpoint-backed credentials require explicit reacquisition.
- Refreshable credentials perform at most one complete recovery attempt.
- Provider HTTP remains isolated and bounded.

---

## Milestone 7 — Rate limiting

### Task 7.1: Implement public plans, rules, keys, windows, and errors

**Files:**

- Create: `concord_core/src/rate_limit/types.rs`
- Create: `concord_core/src/rate_limit/error.rs`
- Modify: `concord_core/src/rate_limit/mod.rs`
- Test: `concord_core/tests/public_api/rate_limit_types.rs`
- Test: `concord_core/tests/security/rate_limit_keys.rs`

**Interfaces produced:**

- inspectable immutable plan/rule/key/window values;
- `RateLimitError`, `RateLimitErrorKind`;
- finite key-size validation.

**Steps:**

- [ ] Add downstream construction and accessor tests.
- [ ] Add oversized-key tests proving no key value appears in errors.
- [ ] Implement exact duplicate rule equality and deterministic order retention.
- [ ] Commit as `feat(rate-limit): add public plan values`.

**Conformance:** `API-RATE-*`, `BOUND-RATE-*`, `SEC-RATE-*`.

### Task 7.2: Implement custom limiter and observer integration

**Files:**

- Create: `concord_core/src/rate_limit/limiter.rs`
- Create: `concord_core/src/rate_limit/observer.rs`
- Test: `concord_core/tests/downstream/custom_limiter.rs`
- Test: `concord_core/tests/runtime/rate_limit_extension.rs`

**Steps:**

- [ ] Add downstream limiter and observer implementations using only advanced APIs.
- [ ] Prove acquire receives visible-attempt metadata and inspectable plan but no headers/body/credentials.
- [ ] Prove observe receives sanitized response head and cannot change current result.
- [ ] Prove authentication recovery acquires again and hidden Reqwest retry does not.
- [ ] Implement `RateLimitPermit::granted()` and the no-op limiter.
- [ ] Commit as `feat(rate-limit): add custom limiter integration`.

**Conformance:** `API-RATE-*`, `EXEC-RATE-*`, `EXEC-RESPHEAD-*`.

### Task 7.3: Implement built-in governor with atomic multi-rule reservation

**Files:**

- Create: `concord_core/src/rate_limit/governor.rs`
- Test: `concord_core/tests/concurrency/governor.rs`
- Property: `concord_core/tests/property/governor_reservation.rs`

**Steps:**

- [ ] Write deterministic tests for one rule, multiple windows, multiple rules, costs, cooldowns, and concurrent callers.
- [ ] Prove no partial token subtraction occurs while any rule still requires waiting.
- [ ] Prove cancellation during wait consumes no quota and no lock is held while sleeping.
- [ ] Implement refill, longest-wait calculation, atomic reservation, LRU/idle eviction, and checked time arithmetic.
- [ ] Prove state entry counts remain within configured limits.
- [ ] Commit as `feat(rate-limit): add bounded local governor`.

**Conformance:** `EXEC-RATE-*`, `BOUND-RATE-*`, `SEC-CLOSED-*`.

### Task 7.4: Implement `Retry-After` observation and future cooldowns

**Files:**

- Create: `concord_core/src/rate_limit/retry_after.rs`
- Test: `concord_core/tests/runtime/retry_after.rs`
- Fuzz: `fuzz/fuzz_targets/retry_after.rs`

**Steps:**

- [ ] Test decimal seconds, HTTP dates, past dates, malformed values, zero, and above-cap values.
- [ ] Prove the current `429` remains `HttpStatus` and is never resent by Concord.
- [ ] Prove future matching attempts wait and longer existing cooldowns are not shortened.
- [ ] Implement current-buckets/host/client scopes.
- [ ] Commit as `feat(rate-limit): add future response cooldowns`.

**Conformance:** `API-RATE-*`, `EXEC-RESPHEAD-*`, `BOUND-RATE-*`.

### Task 7.5: Add DSL declarations and generated construction enforcement

**Files:**

- Create: `concord_macros/src/parse/rate_limit.rs`
- Create: `concord_macros/src/resolve/rate_limit.rs`
- Create: `concord_macros/src/ir/rate_limit.rs`
- Create: `concord_macros/src/codegen/rate_limit.rs`
- Test: `concord_macros/tests/trybuild/pass/rate_limit.rs`
- Test: `concord_macros/tests/trybuild/fail/rate_limit/*`
- Test: `concord_macros/tests/generated_runtime/rate_limit.rs`

**Steps:**

- [ ] Parse flat templates, direct local rules, observer, append, replace, and off.
- [ ] Validate required visible non-secret dimensions, costs, windows, and duplicate dimensions.
- [ ] Generate immutable plan values and exact duplicate elimination.
- [ ] Mark generated clients with statically known rate-limit use so builder construction can require a functional/explicit no-op limiter without governor.
- [ ] Commit as `feat(macros): generate rate-limit plans`.

**Conformance:** `DSL-RATE-*`, `GEN-BUILD-*`, `FEAT-GOV-*`.

### Task 7.6: Add governor feature-boundary matrix

**Files:**

- Test: `concord_core/tests/feature_boundary/governor.rs`
- Test: `concord_examples/tests/no_governor.rs`

**Steps:**

- [ ] Prove default builds select the governor.
- [ ] Prove no-governor generated clients with declared rules fail build without a custom/no-op limiter.
- [ ] Prove an empty plan succeeds under the fallback limiter.
- [ ] Prove a prepared endpoint introducing a nonempty plan fails at acquire rather than silently proceeding.
- [ ] Commit as `test(rate-limit): cover governor feature boundaries`.

### Milestone 7 exit criteria

- Custom and built-in limiter workflows compile and execute.
- Atomic multi-rule reservation and cancellation guarantees pass deterministically.
- Response observation affects only future attempts.
- Key and state bounds are enforced without value leakage.
- Governor absence fails closed.

---

## Milestone 8 — Pagination

### Task 8.1: Implement public page-limit and controller APIs

**Files:**

- Create: `concord_core/src/pagination/controller.rs`
- Create: `concord_core/src/pagination/error.rs`
- Modify: `concord_core/src/pagination/mod.rs`
- Test: `concord_core/tests/public_api/pagination.rs`

**Steps:**

- [ ] Compile-check `PageLimit::Take`, `HardCap`, `From<u32>`, `PaginationController`, and `HasNextCursor`.
- [ ] Prove custom controllers can construct safe typed errors.
- [ ] Document and test `apply` and side-effect-free `next`; expose no public item-count capacity hint.
- [ ] Commit as `feat(pagination): add public controller contracts`.

**Conformance:** `API-PAGE-*`, `BOUND-PAGE-*`.

### Task 8.2: Implement built-in controllers

**Files:**

- Create: `concord_core/src/pagination/offset.rs`
- Create: `concord_core/src/pagination/page.rs`
- Create: `concord_core/src/pagination/cursor.rs`
- Test: `concord_core/tests/runtime/pagination_controllers.rs`

**Steps:**

- [ ] Implement offset and page-number checked arithmetic with exact page capacity.
- [ ] Implement cursor controller using borrowed next cursor and no exact-capacity assumption.
- [ ] Test missing cursor, present empty cursor, overflow, and clone semantics.
- [ ] Commit as `feat(pagination): add built-in controllers`.

### Task 8.3: Implement typed generated-private pagination integration

**Files:**

- Modify: `concord_core/src/__private.rs`
- Create: `concord_core/src/pagination/generated.rs`
- Test: `concord_core/tests/private_contract/paginated_endpoint.rs`

**Interfaces produced:**

- sealed `PaginatedEndpoint` associating endpoint, decoded page, item, and controller types;
- controller application to a fresh endpoint clone.

**Steps:**

- [ ] Implement the minimal generated-private trait without runtime state adapters or `Any`.
- [ ] Add compile checks proving page output only needs `IntoIterator`.
- [ ] Ensure application code cannot implement or name internal controller binding machinery.
- [ ] Commit as `feat(core): add typed pagination integration`.

### Task 8.4: Implement sequential pagination runtime and identity protection

**Files:**

- Create: `concord_core/src/pagination/runtime.rs`
- Create: `concord_core/src/pagination/identity.rs`
- Modify: `concord_core/src/request/terminals.rs`
- Test: `concord_core/tests/runtime/pagination.rs`
- Property: `concord_core/tests/property/pagination_cycles.rs`

**Steps:**

- [ ] Reject requested limits above the configured page ceiling before controller or endpoint planning.
- [ ] Implement true no-op `Take(0)` before controller or endpoint planning.
- [ ] Implement fresh endpoint cloning, controller application, public request preparation, canonical identity insertion, and normal page execution.
- [ ] Compute candidate next state before consuming the page but defer candidate error propagation until universal empty-page and sealed built-in exact-short-page rules are known.
- [ ] Consume items via `IntoIterator`, preserve order, and fail before exceeding the aggregate item ceiling.
- [ ] Implement soft final-page skip of next-state calculation and prove a short custom page continues when `next()` returns `Some`.
- [ ] Implement hard-cap error without sending page `n + 1`.
- [ ] Bound retained identity bytes and fail closed before inserting an identity above the ceiling.
- [ ] Add identical, two-state, and longer-cycle tests proving no repeated request is sent.
- [ ] Add cancellation, page auth recovery, aggregate completion, and no-partial-result tests.
- [ ] Commit as `feat(pagination): execute bounded sequential pages`.

**Conformance:** `EXEC-PAGE-*`, `BOUND-PAGE-*`, `SEC-PAGE-*`.

### Task 8.5: Add DSL pagination and generated bindings

**Files:**

- Create: `concord_macros/src/parse/pagination.rs`
- Create: `concord_macros/src/resolve/pagination.rs`
- Create: `concord_macros/src/ir/pagination.rs`
- Create: `concord_macros/src/codegen/pagination.rs`
- Test: `concord_macros/tests/trybuild/pass/pagination.rs`
- Test: `concord_macros/tests/trybuild/fail/pagination/*`
- Test: `concord_macros/tests/generated_runtime/pagination.rs`

**Steps:**

- [ ] Parse built-in controller bindings and custom controller factory calls with the exact DSL grammar.
- [ ] Validate bodyless/buffered/iterable response requirements.
- [ ] Require exact built-in direct endpoint-field bindings.
- [ ] Validate custom factory expressions, public argument bindings, return types, and safe `PaginationRequest` patch usage through generated compile contracts.
- [ ] Generate `PaginatedEndpoint` implementation without public adapter types.
- [ ] Generate `.paginate(limit).await -> Vec<Item>` and no `.collect()` method.
- [ ] Commit as `feat(macros): generate typed pagination`.

### Task 8.6: Add downstream custom pagination fixture

**Files:**

- Create: `concord_core/tests/downstream/custom_pagination/Cargo.toml`
- Create: `concord_core/tests/downstream/custom_pagination/src/lib.rs`

**Steps:**

- [ ] Implement a custom cursor page using ordinary `IntoIterator`.
- [ ] Implement a custom controller using only `advanced` APIs.
- [ ] Compile a generated endpoint selecting the controller.
- [ ] Prove no `PageItems`, adapter, runtime plan, or private import is required.
- [ ] Commit as `test(pagination): add downstream controller fixture`.

### Milestone 8 exit criteria

- `Vec<T>` and custom iterable pages work directly.
- Pagination has page-limit syntax, operational page/item/identity ceilings, direct await, no collect terminal, no user-selected item cap, and no loop opt-out.
- Natural stop, hard cap, candidate-error precedence, cycle protection, cancellation, and no-partial-result behavior pass.
- No runtime page or controller downcasting exists.

---

## Milestone 9 — Prepared endpoints and public-surface audit

### Task 9.1: Implement prepared endpoint builder foundations

**Files:**

- Create: `concord_core/src/request/prepared.rs`
- Create: `concord_core/src/request/prepared_path.rs`
- Modify: `concord_core/src/request/mod.rs`
- Test: `concord_core/tests/downstream/prepared_endpoint.rs`
- Test: `concord_core/tests/security/prepared_endpoint.rs`

**Interfaces produced:**

- `PreparedEndpoint<R>`;
- validated relative path builder;
- public query/header/body/auth/rate-limit/timeout composition without native Reqwest access.

**Steps:**

- [ ] Add downstream construction tests for GET/POST and buffered/stream response modes.
- [ ] Add path-segment injection and reserved-header collision tests.
- [ ] Add prepared authentication using read-only `CredentialRef<M>` only.
- [ ] Add query/header ordering and timeout tests.
- [ ] Implement sealed endpoint conversion into the same typed execution engine used by generated endpoints.
- [ ] Commit as `feat(core): add prepared endpoints`.

**Conformance:** `API-PREP-*`, `EXEC-HEAD-*`, `EXEC-BODY-*`.

### Task 9.2: Implement prepared response/body/auth/rate-limit selection

**Files:**

- Modify: `concord_core/src/request/prepared.rs`
- Test: `concord_core/tests/runtime/prepared_endpoint_parity.rs`

**Steps:**

- [ ] Add custom request/response codec selection.
- [ ] Add reusable bytes, streams, factories, and multipart bodies.
- [ ] Add bytes, no-content, buffered decoded, streaming, and feature-gated raw terminals.
- [ ] Add multiple compatible auth placements and rate-limit rules.
- [ ] Verify every form enters identical execution phases and limits as generated endpoints.
- [ ] Commit as `feat(core): complete prepared endpoint composition`.

### Task 9.3: Audit stable and unstable exports

**Files:**

- Modify: `concord_core/src/lib.rs`
- Modify: `concord_core/src/__private.rs`
- Modify: `concord_core/src/__development.rs`
- Test: `concord_core/tests/downstream/public_extension/*`
- Test: `concord_core/tests/feature_boundary/private_modules.rs`

**Steps:**

- [ ] Generate an explicit inventory test for `prelude`, `advanced`, and `dangerous` stable items.
- [ ] Compile every documented extension workflow downstream.
- [ ] Prove supported fixtures do not import `__private`.
- [ ] Prove `__development` is feature-gated and unstable.
- [ ] Search public docs/API for execution plans, auth plans, versions, leases, native request/response, pagination adapters, and executor traits; remove any leak.
- [ ] Commit as `refactor(core): finalize public extension boundary`.

**Conformance:** `API-NS-*`, `API-PRIVATE-*`, `COMPAT-SURF-*`.

### Milestone 9 exit criteria

- Prepared endpoints achieve runtime parity without pagination or native transport access.
- Every stable extension workflow compiles in a downstream crate.
- Public namespaces contain only documented stable or dangerous items.
- Generated-private and development-only internals are absent from supported extension code.

---

## Milestone 10 — Documentation, examples, compatibility, and release hardening

### Task 10.1: Compile every specification example

**Files:**

- Create: `concord_examples/src/minimal.rs`
- Create: `concord_examples/src/dsl_reference.rs`
- Create: `concord_examples/src/io.rs`
- Create: `concord_examples/src/auth.rs`
- Create: `concord_examples/src/rate_limit.rs`
- Create: `concord_examples/src/pagination.rs`
- Create: `concord_examples/src/advanced.rs`
- Test: `concord_examples/tests/examples_compile.rs`

**Steps:**

- [ ] Copy every canonical public code example into a compile-checked example or doctest.
- [ ] Ensure feature-gated examples declare their features.
- [ ] Ensure normal examples use deterministic execution and no live credentials.
- [ ] Commit as `docs: compile concord 0.1 examples`.

### Task 10.2: Add a large real-world contract fixture

**Files:**

- Create: `concord_examples/src/riot_like.rs`
- Create: `concord_examples/tests/riot_like.rs`

**Steps:**

- [ ] Model nested regional routing, several credentials, rate-limit templates, optional parameters, request bodies, and pagination.
- [ ] Compile representative facade paths and decode representative DTO fixtures.
- [ ] Keep all runtime tests deterministic and live smoke functions explicitly environment-gated.
- [ ] Commit as `test(examples): add large generated api fixture`.

### Task 10.3: Complete feature and MSRV CI matrix

**Files:**

- Create: `.github/workflows/ci.yml`
- Modify: `justfile`
- Modify: `.config/nextest.toml`

**Steps:**

- [ ] Add default, all-feature, no-default, JSON-only, multipart, each dangerous feature, and dangerous-feature-independence jobs.
- [ ] Use Nextest for unit/integration jobs and `cargo test` only for doctests.
- [ ] Add Rust 1.85 checks for core no-default/all-features, macros, generated fixture, and extension fixture through the pinned toolchain.
- [ ] Add a dependency-floor check proving no selected normal dependency declares a higher MSRV without an explicit specification update.
- [ ] Configure a required Nextest version, inherited default/CI profiles, zero retries, finite
      timeouts, and a bounded group for tests that launch nested Cargo metadata/build checks.
- [ ] Commit as `ci: add feature and msrv matrix`.

**Conformance:** `FEAT-*`, `COMPAT-*`, `CONF-MSRV-001`, `CONF-CI-*`.

### Task 10.4: Complete security sentinel matrix

**Files:**

- Create: `concord_core/tests/security/sentinel_matrix.rs`
- Modify: subsystem security tests

**Steps:**

- [ ] Define unique sentinels for every credential, request/response body, provider body, cursor, rate key, proxy, and native URL category.
- [ ] Recursively inspect display, debug, sources, hooks, debug output, limiter contexts, pagination errors, completion events, and wrapper debug.
- [ ] Add a positive-access fixture proving terminal body sentinels are available only through the explicit complete `StatusBody` accessor.
- [ ] Verify dangerous boundaries only expose data explicitly allowed by their specifications.
- [ ] Commit as `test(security): complete cross-surface sentinel matrix`.

**Conformance:** `SEC-TEST-001`, `CONF-SEC-001`.

### Task 10.5: Add required property and fuzz targets

**Files:**

- Create: `concord_core/tests/property/*`
- Create: `fuzz/Cargo.toml`
- Create: `fuzz/fuzz_targets/*`

**Steps:**

- [ ] Add required path, safe URL, byte-limit, credential sequence, and pagination identity property tests.
- [ ] Add bounded fuzz targets from the Conformance Plan.
- [ ] Configure scheduled long fuzzing and bounded release smoke execution.
- [ ] Commit as `test: add property and fuzz coverage`.

### Task 10.6: Add performance package and benchmark compilation

**Files:**

- Create: `perf/Cargo.toml`
- Create: `perf/src/lib.rs`
- Create: `perf/benches/*`

**Steps:**

- [ ] Benchmark local client/request construction, public head preparation, codec paths, ready credentials, limiter ready path, pagination loop overhead, and stream wrappers.
- [ ] Avoid network latency benchmarks and hard release thresholds.
- [ ] Add `cargo bench --no-run` to the release gate.
- [ ] Commit as `perf: add local runtime benchmarks`.

### Task 10.7: Enable strict docs, lints, dependency, and release governance

**Files:**

- Modify: crate roots
- Modify: `deny.toml`
- Modify: `justfile`
- Modify: coverage plan, evidence catalog, and release report

**Steps:**

- [ ] Enable `#![deny(missing_docs)]` for stable public surfaces.
- [ ] Deny warnings in Clippy and Rustdoc.
- [ ] Add Cargo Deny license/advisory/source policy.
- [ ] Record manual normative ownership, local-link, coverage-plan, and evidence-catalog review.
- [ ] Compose `just release` from maintained default/all/no-default recipes; add `git diff --check`
      and benchmark compilation without duplicating recipe command bodies.
- [ ] Commit as `chore: enable concord release gates`.

### Task 10.8: Finalize conformance evidence and normative status

**Files:**

- Modify: `conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`
- Modify: `conformance/CONFORMANCE_EVIDENCE_0.1.csv`
- Modify: normative document status headers
- Modify: `README.md`

**Steps:**

- [ ] Verify every normative requirement retains exactly one coverage-plan row.
- [ ] Add one or more exact actual-evidence rows for every requirement, each identifying a test, command, or approved manual procedure.
- [ ] Correct evidence classifications, including compile-fail diagnostics as `compile-fail`.
- [ ] Verify every actual result is `passing` or approved `waived` at the release commit.
- [ ] Run the complete release command on a clean checkout.
- [ ] Change normative documents to `Normative Release Final` only after all gates pass.
- [ ] Change architecture documents to `Internal Release Approved` only after reconciliation with the conforming implementation.
- [ ] Commit as `release: finalize concord 0.1 conformance evidence`.

### Milestone 10 exit criteria

- Every normative requirement has exact passing or explicitly approved waived actual evidence; coverage-plan rows alone are insufficient.
- Every public example compiles.
- Default, all-feature, no-default, dangerous-feature, and MSRV matrices pass.
- Security, property, fuzz-smoke, docs, dependency, and benchmark gates pass.
- `just release` succeeds on a clean checkout.

---

## 2. Commit and review policy

Each task should produce one independently reviewable commit or a small sequence of test/implementation commits.

Recommended pattern:

```text
test: add failing <behavior> coverage
feat: implement <behavior>
docs: document <public surface>
```

Before merging a milestone:

1. run focused tests;
2. run full default checks;
3. run relevant feature checks;
4. review public API diff;
5. review coverage-plan and actual-evidence updates;
6. review security sentinels;
7. confirm no temporary public API remains.

## 3. Integration discipline

- Never introduce an obsolete API as a temporary compatibility shim.
- Never expose a private type to unblock macro code; add a minimal generated-private adapter instead.
- Never duplicate execution logic in generated code.
- Never bypass deterministic runtime tests with direct private state assertions when public behavior can be observed.
- Never merge a feature whose normative documentation, public API, and conformance evidence disagree.

## 4. Final implementation definition of done

The project implementation is complete when:

- the canonical release gate passes;
- requirement coverage planning is exact and actual evidence is complete;
- every stable public item is documented;
- every dangerous item names reduced guarantees;
- no runtime response or page uses `Any` downcasting;
- no production transport/executor trait is public;
- no cache, general retry loop, profile, or policy-bundle subsystem exists;
- all examples and downstream fixtures compile;
- security and concurrency matrices pass deterministically.
