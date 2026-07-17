# Phase 2 Integrated Conformance Report

## Verdict

`PHASE_2_COMPLETE_WITH_LIMITATIONS`

All 26 Phase 2 exit criteria are satisfied after the integrated-audit correction. The generated
explicit-endpoint ownership defect is fixed at compile time. The limitations recorded below are
the authorized Phase 2 product boundary, explicitly partial evidence for broader 0.1 requirements,
and one review-patch process limitation; none is a remaining Phase 2 source defect or invalidates
an exit criterion.

This is not a claim of full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable evidence identity

- Starting baseline: `5331b2c39b4c5911bbebfbc52b61ccc342d47deb`
- Original Phase 2 source commit tested: `055d74718c443412a605bd880cfad9415a824f99`
- Corrected final Phase 2 source commit tested: `ea1ce944fe1d2622db7a66600d734ac0dc04784f`
- Corrected validation time recorded for evidence: `2026-07-17T16:01:17Z`
- Corrected evidence/report commit: recorded by the documentation-only evidence anchor
- Corrected evidence anchor commit: this report's documentation-only follow-up
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0 from `rust-toolchain.toml`
- Nextest exercised: 0.9.140; repository minimum: 0.9.115
- Managed HTTP baseline: Reqwest 0.13.4 with its default features disabled

The tracked source tree was clean before corrected integrated validation. An unrelated untracked
workspace archive, `concord_api.zip`, appeared during source work and was excluded from every
commit and patch; it was no longer present at evidence review. The report/evidence commit follows
the tested source commit, and the final anchor changes documentation only. No evidence row points
to a documentation commit.

## Phase 2 commits

| Unit | Commit | Subject | Purpose |
|---|---|---|---|
| Contract decisions | `1f5ca764f8f21c622ef9f323bbeb2188bc16e1ed` | `docs: resolve phase2 lifecycle and request contracts` | Clarified deadline/completion ownership and stable request-wrapper paths before Rust changes. |
| PR 2.1 | `94fb194d4904c104f6cc59f8ffccc1056dfb4ea0` | `feat(core): add request lifecycle contracts` | Added final hooks, debug levels, operation identity, completion metadata, response metadata, and runtime selections. |
| PR 2.2 | `a6d9d47a9bbdef5eed2f87376b7ca19f096b2086` | `feat(core): add typed request preparation` | Added the private shared client core, stable pending wrappers, generated-private endpoint bridge, and lazy safe head preparation. |
| PR 2.3 | `e5dcd2c74ad31f0268ce0d6525335db3c401b6c2` | `feat(core): execute typed bodyless requests` | Replaced the proof executor with one production Bytes/NoContent lifecycle and managed Reqwest path. |
| PR 2.4 | `055d74718c443412a605bd880cfad9415a824f99` | `feat: deliver first executable generated endpoint` | Connected the existing parser/model/codegen pipeline to the production request path and contract version 2. |
| Integrated audit correction | `ea1ce944fe1d2622db7a66600d734ac0dc04784f` | `fix(macros): bind endpoints to generated clients` | Added each generated API's sealed endpoint capability, compile-time ownership rejection, public constructor-name cleanup, complete endpoint documentation identity, and focused contract clarification. |

## Contract decisions applied

Completion observation is outside the operation deadline. The deadline covers work that can
determine the application result: `before_send`, managed native execution, `on_response_head`,
successful Bytes collection, and terminal status-body collection. Once the result is fixed,
`on_complete` is awaited without resetting or extending that deadline. It cannot replace the fixed
result. `CompletionEvent::elapsed` ends at result fixation and excludes completion-hook time.
Ordinary timeout results still deliver `CompletionOutcome::Timeout` while polling continues;
cancellation or panic may prevent completion delivery.

The stable application wrapper paths are:

```rust
concord_core::Request<E>
concord_core::ResponseRequest<E>
concord_core::prelude::Request<E>
concord_core::prelude::ResponseRequest<E>
```

Their fields and construction remain private. Generated implementation uses the narrow
`concord_core::__private` bridge, but generated public signatures and Rustdoc do not expose that
path. The named public `BoxFuture` alias keeps `IntoFuture::IntoFuture` publicly nameable.

No unresolved normative contradiction remained after decision commit
`1f5ca764f8f21c622ef9f323bbeb2188bc16e1ed`.

## Public surface delivered

Phase 2 adds or finalizes:

- `BoxFuture<'a, T>`;
- `DebugLevel`, `HookError`, and opaque `OperationId`;
- `BodyKind`, `BeforeSend`, `ResponseHeadEvent`, `CompletionEvent`,
  `PaginationCompletionEvent`, and `CompletionOutcome`;
- `RuntimeHooks` with no-op defaults and immutable `RuntimeConfigBuilder::hooks` / `debug`
  selection;
- `ExecutionMeta`, `DecodedResponse<T>`, and `NoContent`;
- opaque root/prelude `Request<E>` and `ResponseRequest<E>` wrappers;
- request-local `timeout`, `disable_timeout`, `inherit_timeout`, `debug`, and `response` methods;
- direct `IntoFuture` for valid generated Bytes and NoContent endpoint types.

The pagination completion event exists because it is part of the final hook trait. Pagination
execution was not added.

## Generated surface delivered

The existing `api!` compiler now supports the authorized Phase 2 subset: one fixed-base client per
invocation, one scope, required scope and endpoint parameters, static/dynamic path atoms, multiple
bodyless GET endpoints, path-block or semicolon form, and `Bytes` / `NoContent` responses.

Generated APIs provide:

- a concrete cheap-clone client owning one private shared core;
- fallible no-input `new()` and a public builder with immediate `.http(F)` and `.runtime(F)`;
- structured duplicate-call/build errors without invoking duplicate closures;
- a scope facade that owns immutable values;
- explicit endpoint constructors that require all enclosing scope and endpoint inputs and expose
  sanitized original DSL parameter names rather than collision-resistant storage identifiers;
- one public, documentation-hidden, API-specific endpoint capability with a private seal;
- facade methods and `client.request(endpoint)` using the same capability-constrained path and
  returning the stable `Request<E>` wrapper;
- direct `.await`, `.response().await`, and request-local timeout/debug setters;
- generated contract version 2 with compile-time mismatch detection.

Only endpoint types emitted for a generated API implement that API's capability. A foreign
generated endpoint, an arbitrary application value, and a downstream attempted implementation are
all compile-time errors. Facade methods construct their endpoint value and call the same generated
client request path, so they cannot bypass the ownership check. The public bound names only the
API-specific capability; generated Rustdoc exposes neither its private seal nor
`concord_core::__private`. Facade and endpoint documentation use the same complete
`client::scope::endpoint` identity.

Dynamic values are prepared only on first poll and appended through the URL library's structured
path-segment API. Slash, backslash, dot, dot-dot, empty, and pre-encoded injection cases cannot
create an additional segment. Fixed-base/configuration errors are reported during client build.

## Production execution lifecycle

The one production bodyless path is:

```text
first poll: allocate OperationId and start instant
→ resolve one operation deadline
→ prepare safe public head and Empty body metadata
→ before_send under remaining budget
→ materialize Empty body and build native request
→ sanitized request debug
→ one ApplicationHttp managed Reqwest call
→ sanitized response-head debug
→ on_response_head under remaining budget
→ classify status
→ bounded successful Bytes, unpolled NoContent, or complete-only terminal capture
→ fix direct/DecodedResponse result, outcome, final status, and elapsed
→ await on_complete outside the operation deadline
→ return the unchanged fixed result
```

TLS/`https_only`, route, and public-head failures occur before hooks and native activity.
`before_send` veto prevents native execution. Every received final response is observed before
status/body handling. Every non-2xx, including 3xx and 429, uses bounded complete-only terminal
capture and remains `ApiError::HttpStatus`; read failure, overflow, and timeout preserve status.

Successful Bytes use the independent buffered-response limit: known excess fails before polling,
unknown overflow withholds the first excess frame and all prefixes, read failure becomes
`ResponseBody`, and timeout becomes `Timeout`. NoContent accepts every 2xx and drops the native
response without polling its body.

## Proof migration and retained models

Removed or replaced:

- the inert eager `RequestHead` and its generated integration;
- the `cfg(test)` typed executor loop and its proof-only operation ID/deadline/response handling;
- the constant infallible generated constructor and inert endpoint result;
- historical `PR 0.2 accepts...` parser diagnostics;
- generated contract version 1 compatibility aliases.

`concord_core/src/execute/mod.rs` is now the production executor, and
`concord_core/src/execute/tests.rs` tests that executor rather than hosting a parallel proof loop.
The reduced `cfg(test)` auth and pagination models remain only for independent future state-machine
and ceiling invariants; they do not duplicate a Phase 2 production representation.

## Test architecture

Deterministic boundary tests use private request captures, gates, lifecycle counters, and scripted
responses to prove exact metadata, stage ordering, failure cutoffs, a single non-resetting deadline,
operation-ID correlation, completion counts, and cancellation behavior. They do not introduce a
public transport or production executor trait.

Real tests use test-owned ephemeral local HTTP servers and an explicit local proxy. They prove
actual Reqwest GET execution, generated fixed-base routing without a contested port, bounded Bytes,
NoContent body non-polling, terminal capture, connection/body failure behavior, and HTTP/TLS
preflight. No external service or credential is used.

Trybuild owns source-only generated diagnostics. Standalone Cargo fixtures remain only for renamed,
missing, or incompatible dependency topology. Nextest is the only unit/integration runner;
`cargo test` appears only in maintained doctest recipes.

Sentinel tests cover `Display`, `Debug`, recursive error sources, hook contexts, built-in debug,
deterministic mismatch diagnostics, terminal status wrappers, and successful response wrapper
`Debug`. Body bytes, header values, credentials, native authenticated URLs, and proxy credentials
do not reach automatic surfaces.

## Per-PR validation

Each source PR generated an ignored uncommitted review patch and a committed binary patch. The
PR 2.3 uncommitted review patch omitted one newly created response-handler file. The complete
committed patch and final integrated source were subsequently reviewed and validated. The other
source review patches included their complete tracked source changes.

| Unit | Focused acceptance | Maintained validation |
|---|---|---|
| Contract decisions | requirement/coverage-row and affected-doc manual inspection | `git diff --check` |
| PR 2.1 | lifecycle/public-path suite; final integrated rerun 11/11 | format, check/default+all, Clippy/default+all, Nextest/default+all, doctests, Rustdoc, diff check |
| PR 2.2 | request preparation/identity/route suite; final integrated rerun 6/6 | same maintained source-PR matrix |
| PR 2.3 | production executor/deadline/status/Bytes/NoContent suite; final integrated rerun 15/15 | same maintained source-PR matrix |
| PR 2.4 | generated contract/routing 3/3; macro generated/fixture/Trybuild package 20/20 | same maintained source-PR matrix |
| Integrated audit correction | API ownership compile failures 3/3; generated package 26/26; generated Rustdoc and constructor-name inspection | format, check/default+all, Clippy/default+all, Rustdoc/default+all, diff check, default/all/no-default release matrix |

The cumulative PR 2.4 patch was amended before final validation to strengthen two independent
oracles: non-`Clone` facade input/sibling-request isolation and a slash/backslash/pre-encoded/
empty/dot/dot-dot/static-separator route matrix.

The correction patch was reviewed in full before commit. Its compile-fail oracles independently
reject a foreign generated endpoint, an arbitrary value, and a downstream forged capability
implementation. The Rustdoc oracle verifies the public capability bound while rejecting the
private seal, `concord_core::__private`, `ClientCore`, `EndpointSpec`, and `ResponseHandler`.

## Integrated validation

Every maintained public recipe passed against exact source commit
`ea1ce944fe1d2622db7a66600d734ac0dc04784f` through the post-commit `just release`
gate:

| Recipe | Result |
|---|---|
| `just fmt-check` | passed |
| `just check` | passed; workspace all targets, default features |
| `just check-all` | passed; workspace all targets, all features |
| `just check-no-default` | passed; core all targets without defaults |
| `just clippy` | passed with warnings denied |
| `just clippy-all` | passed with warnings denied |
| `just test` | passed; Nextest 268/268 across 31 binaries |
| `just test-all` | passed; CI-profile Nextest 271/271 across 31 binaries |
| `just test-no-default` | passed; exact private HTTP/TLS boundary 1/1 |
| `just doctest` | passed; core 2/2, other libraries had zero doctests |
| `just doctest-all` | passed; core 2/2, other libraries had zero doctests |
| `just doc` | passed for all three libraries with warnings denied |
| `just doc-all` | passed for all three libraries with warnings denied |
| `just deny` | advisories, bans, licenses, and sources passed |
| `just diff-check` | passed |
| `just release` | passed the composed default/all/no-default matrix independently |

Cargo Deny emitted only the accepted non-failing duplicate-version warnings for
`core-foundation` and `windows-sys`.

Focused original-source commands remained covered by the corrected full matrix. Correction-specific
commands also passed:

- `just test`: 268/268;
- `cargo nextest run -p concord_macros`: 26/26, including renamed dependency, contract mismatch,
  generated public Rustdoc, and six focused Trybuild cases;
- `just release`: default 268/268, all-feature 271/271, no-default 1/1, plus the maintained
  check, Clippy, doctest, Rustdoc, Cargo Deny, and diff-check gates.

No Python repository script, Python test, specification checker, replacement checker, nested
ordinary `cargo test`, external service, or Phase 3 command was run.

## Requirement evidence

`docs/conformance/CONFORMANCE_EVIDENCE_0.1.csv` contains 178 Phase 2 rows: one exact requirement ID
per row and one unique `P2-*` evidence ID per row. The original 170 rows retain tested source commit
`055d74718c443412a605bd880cfad9415a824f99`; eight correction rows point to corrected tested source
commit `ea1ce944fe1d2622db7a66600d734ac0dc04784f`. There are 114 direct rows and 64 rows explicitly
classified as partial. Historical Phase 0 and Phase 1 rows were not rewritten.

The integrated audit reclassified `EXEC-CANCEL-001`, `GEN-NAME-001`, and every other reviewed
Phase 2 direct row whose owning 0.1 requirement explicitly includes unimplemented later categories.
Those partial rows now identify the exact direct Phase 2 portion and the unproved body, credential,
provider, limiter, authentication, prepared-endpoint, streaming, decoder, dynamic-host, or
pagination portion. Structural requirements whose complete public configuration or negative shape
is already present remain direct. No waiver is used.

The evidence combines public compile/Rustdoc tests, deterministic execution tests, real local
Reqwest tests, redaction/property tests, dependency/feature tests, and the complete maintained
release command. Planned coverage rows remain planning, not evidence.

## Cumulative architecture audit

The final source audit found exactly one production representation for pending request, prepared
head, operation ID, operation deadline, basic execution, successful Bytes collection, and
NoContent handling. It also confirmed one generated, API-specific sealed capability per client,
with facade and explicit construction sharing its request path. It confirmed the absence of:

- an inert/eager generated `RequestHead`;
- a duplicate test-only execution loop;
- runtime `Any` or boxed decoded values;
- a public transport, production executor trait, or raw Reqwest client/request/response;
- eager generated URL construction;
- generated public `__private` paths;
- cross-client explicit-endpoint admission, arbitrary endpoint admission, or a downstream-open
  generated endpoint capability;
- stale PR 0.2 diagnostics;
- authentication, limiter, request-body, pagination-runtime, streaming, multipart, raw-success,
  or other Phase 3 placeholders;
- Python repository tooling.

The feature-gated `dangerous::ReqwestRetryBuilder` remains the previously approved Phase 1 native
retry-policy opt-in. It does not expose a Reqwest client, request, response, or transport boundary.

## Remaining limitations

Phase 2 intentionally supports only empty bodyless GET requests, `Bytes`, and `NoContent`; one
fixed-base client and one scope per macro invocation; required path parameters; and path-only or
semicolon endpoints. It does not implement text/JSON codecs, request bodies, auth, limiting,
pagination execution, streaming, multipart, query/header/host-prefix clauses, custom methods,
prepared endpoints, dangerous raw success, or the complete nested value system.

Completion hooks are awaited and extension-owned; a non-returning hook can postpone delivery of an
already-fixed result. Cancellation or panic may prevent completion delivery. These are approved
contract semantics, not missing Phase 2 behavior.

The PR 2.3 uncommitted review patch omitted `concord_core/src/response/handler.rs` while that file
was newly untracked. The committed PR 2.3 patch contained it, and both the complete committed patch
and final integrated source were subsequently reviewed and validated. This is a recorded process
limitation, not a source defect.

The feature-gated deterministic development boundary still has no approved generated-client
injection method. Before streaming is implemented, `Request<E>::response()` must become
capability-constrained so it is absent for streaming endpoints. Built-in textual debug remains
sanitized but does not include the operation ID. These are future contract/publication concerns,
not Phase 2 exit failures.

## Phase 3 readiness

The repository now has the single executable generated request path required as the base for later
work. Any next phase requires a separate approved contract map and mission and must extend this
path without adding parallel request, head, deadline, or executor representations.

`PHASE_3_NOT_STARTED`
