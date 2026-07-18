# Phase 5 Integrated Conformance Report

## Verdict

`PHASE_5_COMPLETE_WITH_LIMITATIONS`

All applicable Phase 5 exit criteria are satisfied. The limitations below are clauses of compound
0.1 requirements whose remaining behavior belongs to Phases 6-10; none invalidates the completed
Phase 5 value, compiler, generated API, or request-shaping boundary.

`PHASE_6_NOT_STARTED`
This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable evidence identity

- Starting HEAD and Phase 4 corrected anchor:
  `16cae134e1dc6681bacaa3c80fb37cc0ff8ed6e5`
- Branch: `master`
- Initial working tree: clean
- Phase 4 immutable product source:
  `6472bc01e55bbd4282a8d5e06deeb9126650f9c4`
- Phase 4 corrected source/test commit:
  `909021e7b82436bdfdff1e1ffea67f3255e63c77`
- Phase 4 complete evidence state:
  `705ea9b453ae7434cf3a2aba7693147eefb8348a`
- Phase 4 corrected anchor:
  `16cae134e1dc6681bacaa3c80fb37cc0ff8ed6e5`
- Approved request-setter decision commit:
  `255e1fd94f4614b5a3c02cda0f820c4eb56146c5`
- Immutable tested Phase 5 source:
  `c513710c1e6ae2333a58ec481c2b74a4a0529a5f`
- Complete Phase 5 evidence-state commit:
  `a8353a151a35b3ba8a3d8ebd689ad269e7b1d948`
- Phase 5 evidence anchor: the later documentation-only `docs: anchor phase5 evidence` commit; its
  object identity is intentionally external to its own content
- Validation/evidence time: `2026-07-18T09:46:11Z`
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Cargo resolver: 3
- Nextest exercised: 0.9.140; repository minimum 0.9.115
- Managed HTTP baseline: Reqwest 0.13.4 with default features disabled

The complete maintained validation matrix ran against the immutable source commit before any
report, coverage, evidence, or Program changes. The later evidence-state commit contains all 203
Phase 5 evidence IDs. Its five audit rows temporarily use the non-self-referential pending marker;
the subsequent documentation-only anchor changes only that marker and the corresponding audit
commands/identity text. It adds no IDs, changes no classification, and contains no product source.

All five final commit-scoped audit commands pass against
`a8353a151a35b3ba8a3d8ebd689ad269e7b1d948`.

## Phase 5 commits

| Unit | Commit | Subject | Durable result |
|---|---|---|---|
| 5.0 | `255e1fd94f4614b5a3c02cda0f820c4eb56146c5` | `docs: resolve generated request setter contract` | Approved sealed generated extension traits, one generated prelude, deterministic aliases, and one private endpoint-map bridge. |
| 5.1 | `a697d80b472123682290183879326d2a5336485e` | `feat(core): add request head shaping primitives` | Evolved the one `HeadBuilder` for validated host, path, ordered query, public headers, safe URL finalization, and endpoint mapping. |
| 5.1 correction | `4bcb50140c58a97a15c635fd21e84b43cf83c04c` | `feat(core): complete Phase 5 media and retry support` | Completed media ownership, multipart conflict behavior, and lazy selected retry factory semantics with HTTP override precedence. |
| 5.2-5.6 | `c513710c1e6ae2333a58ec481c2b74a4a0529a5f` | `feat(macros): generate complete Phase 5 APIs` | Replaced the flat compiler model in place with source AST, resolution, normalized IR, naming, snapshots, nested generated APIs, extension traits/preludes, and integrated first-poll shaping. |

Each unit received focused validation, a complete uncommitted patch capture, full patch inspection,
correction where necessary, rerun, a coherent commit, and a committed patch under ignored
`git_diff/`.

## Approved request-setter contract

For each endpoint with optional/defaulted endpoint controls, code generation emits a public sealed
extension trait implemented only for `concord_core::Request<ThatEndpoint>`. The generated client
support module contains a public `prelude` that re-exports each endpoint trait under a deterministic
alias containing client identity, full scope path, endpoint public name/alias, and `RequestExt`.
One application import enables every endpoint setter for that client:

```rust
use example_api::prelude::*;
```

Facade methods still return exactly `concord_core::Request<E>`. Optional setters store `Some`,
clearers store `None`, default setters replace the selected value, and resetters restore the value
retained in the endpoint snapshot without re-evaluating the default. Methods consume and return the
same request type. The single doc-hidden core `map_endpoint` bridge preserves the client core,
request-local timeout/debug options, body, response handler, endpoint ownership, and unpolled state.
It invokes no hook, codec, factory, body materialization, URL preparation, or HTTP.

The generated trait is sealed by generated private implementation detail and cannot be implemented
downstream. No private core trait or `concord_core::__private` path appears in public signatures or
rendered Rustdoc. Explicit endpoint values retain inherent controls. Generic
`timeout`, `disable_timeout`, `inherit_timeout`, and `debug` setters remain inherent core methods.
Direct await and metadata terminals retain their existing capability boundaries.

## Contract map and authority review

The complete map is the ignored artifact `git_diff/phase5-contract-map.md`. It records the clean
baseline, decision commit, exact signatures and exports, 198 exact known requirement IDs,
contradiction resolutions, source migration, test oracles, and parallel-architecture prevention.

The exact applicability inventory is:

- product: `PROD-CORE-001`, `PROD-CORE-002`, `PROD-CORE-003`, `PROD-CORE-004`,
  `PROD-MODEL-001`, `PROD-MODEL-002`, `PROD-MODEL-006`, `PROD-GUAR-001`,
  `PROD-GUAR-002`, `PROD-GUAR-003`, `PROD-GUAR-010`, `PROD-GUAR-013`,
  `PROD-STATE-001`, `PROD-STATE-003`, `PROD-SURF-002`;
- DSL lexical/value/expression: `DSL-LEX-001`, `DSL-LEX-002`, `DSL-LEX-003`,
  `DSL-CLIENT-001`, `DSL-TOP-001`, `DSL-TOP-002`, `DSL-ORDER-001`, `DSL-ORDER-002`,
  `DSL-BASE-001`, `DSL-BASE-002`, `DSL-BASE-003`, `DSL-VAL-001`, `DSL-VAL-002`,
  `DSL-VAL-003`, `DSL-VAL-004`, `DSL-VAL-005`, `DSL-EXPR-001`, `DSL-EXPR-002`,
  `DSL-EXPR-003`;
- DSL shaping: `DSL-RETRY-001`, `DSL-RETRY-002`, `DSL-RETRY-003`, `DSL-HOST-001`,
  `DSL-HOST-002`, `DSL-HOST-003`, `DSL-HOST-004`, `DSL-HOST-005`, `DSL-PATH-001`,
  `DSL-PATH-002`, `DSL-PATH-003`, `DSL-PATH-004`, `DSL-PATH-005`, `DSL-PATH-006`,
  `DSL-QUERY-001`, `DSL-QUERY-002`, `DSL-QUERY-003`, `DSL-QUERY-004`, `DSL-HEAD-001`,
  `DSL-HEAD-002`, `DSL-HEAD-003`, `DSL-HEAD-004`, `DSL-TIME-001`, `DSL-TIME-002`;
- DSL scopes/endpoints: `DSL-SCOPE-001`, `DSL-SCOPE-002`, `DSL-SCOPE-003`,
  `DSL-END-001`, `DSL-END-002`, `DSL-END-003`, `DSL-END-004`, `DSL-END-005`,
  `DSL-PARAM-001`, `DSL-PARAM-002`, `DSL-NAME-001`, `DSL-NAME-002`, `DSL-NORM-001`,
  `DSL-FEAT-003`;
- generated API: `GEN-TYPE-001`, `GEN-TYPE-002`, `GEN-TYPE-003`, `GEN-CTOR-001`,
  `GEN-CTOR-002`, `GEN-CONFIG-001`, `GEN-CONFIG-002`, `GEN-CONFIG-003`,
  `GEN-BUILD-001`, `GEN-BUILD-002`, `GEN-BUILD-003`, `GEN-BUILD-005`,
  `GEN-CLONE-001`, `GEN-CLONE-002`, `GEN-CLONE-003`, `GEN-FACADE-001`,
  `GEN-END-001`, `GEN-END-002`, `GEN-END-003`, `GEN-REQ-001`, `GEN-REQ-002`,
  `GEN-DEFAULT-001`, `GEN-TERM-001`, `GEN-TERM-002`, `GEN-TERM-003`, `GEN-EXPL-001`,
  `GEN-EXPL-002`, `GEN-EXPL-003`, `GEN-NAME-001`, `GEN-NAME-002`, `GEN-OWN-001`,
  `GEN-OWN-002`, `GEN-DOC-001`, `GEN-DOC-002`;
- public/security/bounds: `API-NS-001`, `API-REQ-001`, `API-REQ-002`,
  `API-CODEC-003`, `API-PRIVATE-001`, `API-PRIVATE-002`, `SEC-DATA-001`,
  `SEC-SECRET-001`, `SEC-SECRET-002`, `SEC-URL-001`, `SEC-URL-002`, `SEC-URL-003`,
  `SEC-HEAD-001`, `SEC-HEAD-002`, `SEC-HEAD-003`, `SEC-QUERY-001`, `SEC-FAIL-001`,
  `SEC-TEST-001`, `BOUND-COMPILE-001`, `BOUND-COMPILE-002`, `BOUND-MATH-001`,
  `BOUND-MATH-002`;
- execution: `EXEC-BUILD-002`, `EXEC-CONSTRUCT-001`, `EXEC-CONSTRUCT-002`,
  `EXEC-CONSTRUCT-003`, `EXEC-CREATE-001`, `EXEC-CREATE-002`, `EXEC-ORDER-001`,
  `EXEC-HEAD-001`, `EXEC-HEAD-002`, `EXEC-BODYMETA-001`, `EXEC-HOOK-001`,
  `EXEC-HOOK-002`, `EXEC-NATIVE-001`, `EXEC-NATIVE-002`, `EXEC-DEADLINE-001`,
  `EXEC-DEADLINE-004`, `EXEC-COMPLETE-001`, `EXEC-COMPLETE-005`, `EXEC-PREC-001`,
  `EXEC-CANCEL-001`, `EXEC-COUNT-001`, `EXEC-COUNT-002`, `EXEC-COUNT-003`;
- features/compatibility: `COMPAT-CRATE-001`, `COMPAT-RUST-001`, `COMPAT-RUST-002`,
  `COMPAT-REQWEST-001`, `COMPAT-REQWEST-002`, `COMPAT-REQWEST-003`, `FEAT-CORE-001`,
  `FEAT-CORE-002`, `FEAT-NODEFAULT-001`, `FEAT-NODEFAULT-002`, `FEAT-DANGER-001`,
  `FEAT-DANGER-002`, `FEAT-DEV-001`, `COMPAT-MACRO-001`, `COMPAT-MACRO-002`,
  `COMPAT-MACRO-003`, `FEAT-DIAG-001`, `COMPAT-GEN-001`, `COMPAT-GEN-002`,
  `COMPAT-SURF-001`, `COMPAT-BUILD-001`, `COMPAT-DEP-001`, `COMPAT-DEP-002`,
  `COMPAT-DEPS-001`, `FEAT-DOC-001`;
- conformance: `CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`,
  `CONF-DOWNSTREAM-001`, `CONF-DOWNSTREAM-002`, `CONF-DET-001`, `CONF-TRACE-001`,
  `CONF-TRACE-005`, `CONF-TRACE-006`, `CONF-PARSER-001`, `CONF-SEMA-001`,
  `CONF-REQWEST-001`, `CONF-ORDER-001`, `CONF-SEC-001`, `CONF-MSRV-001`,
  `CONF-DOC-001`, `CONF-TEST-001`, `CONF-CI-001`, `CONF-CI-002`, `CONF-CI-003`,
  `CONF-CI-004`, `CONF-VERTICAL-001`.

No strategic contradiction remained after the approved setter decision. Defaults are restricted
source expressions whose selected values are evaluated at the owning snapshot boundary. Optional
and vector shapes are syntactically recognized. Host prefixes operate only on DNS hosts while
preserving scheme/port/base path. Path replace/off never removes the fixed base path. Query order
is held in a sequence and serialized once. Public headers use the one `PreparedHead` map before
media insertion. Retry factories are lazy so an HTTP override suppresses an overridden DSL
factory. Trait/prelude names contain complete semantic identity and are collision-checked.

## Compiler architecture

The production macro is one pipeline:

```text
tokens
→ source AST (`ast.rs`, `parse.rs`)
→ lexical symbol tables and semantic identities (`resolve.rs`)
→ resolved public references
→ normalized inherited client/scope/endpoint IR
→ deterministic naming service (`names.rs`)
→ direct typed code generation (`codegen.rs`)
```

Normalization walks each client tree outer-to-inner and resolves host/path/query/header/timeout
inherit, append, replace, removal, and off semantics. The IR contains no lexical references or
unresolved layer operation. Code generation emits immutable typed snapshots and direct preparation
code; it emits no runtime API tree or operation interpreter. The prior flat model was replaced in
place, not retained as a parallel parser/codegen route. Existing body and response mode selection
was migrated into the same IR and generator. Generated contract version 6 is synchronized between
macro and core.

## Client values and builders

One invocation may declare multiple uniquely named clients, each with its own concrete type,
builder, support module, endpoint capability, `ClientCore`, and immutable public-value snapshot.
Required client values are convenience-constructor arguments in DSL order and required builder
inputs. Optional values have setters/clearers. Defaulted values have setters/resetters. Repeated
setters replace the prior selection. Missing required input returns
`ClientBuildErrorKind::MissingInput` without embedding a user value in diagnostic text.

An overridden client default is not evaluated. A selected/reset default evaluates exactly once in
`build()` and is retained in the immutable `Arc` snapshot. Client clones share the same snapshot
and runtime without user-value `Clone` bounds; independently built clients receive independent
snapshots and evaluate selected defaults independently. No post-build client-value mutation method
is generated. HTTP/runtime closures remain immediate and duplicate calls suppress the duplicate
closure. Omitted/off retry is disabled, Reqwest default is explicit, dangerous custom retry is
feature-gated, and `.http(...)` replacement prevents an overridden custom factory from running.

## Nested scopes and facades

Clients may contain root endpoints and sibling/nested scopes. Nesting is accepted through depth 64
and rejected at 65. Scope entry takes required local values in declaration order, resolves optional
values to `None`, evaluates selected local defaults once, captures enclosing snapshots, and performs
no request work. Nested entry is generated on the enclosing facade.

Optional/defaulted facade controls consume and return a new immutable facade. The source facade is
unchanged. Reset restores the retained resolved default without evaluating it again. Arc-backed
scope frames avoid unnecessary user-value `Clone` bounds and preserve outer-to-inner lexical
visibility. Ambiguous visible scope names fail during semantic resolution.

## Endpoint values, requests, and explicit parity

Facade endpoint methods take required endpoint values in declaration order and keep the body last.
Optional/defaulted endpoint values are captured in the new endpoint snapshot; selected defaults
evaluate once when that pending request is created. Request extension setters map only the endpoint
snapshot, preserve siblings and request-local options, and do no shaping or execution.

Explicit endpoint constructors accept required enclosing scope values outermost-to-innermost,
required endpoint values in declaration order, then the body. They provide inherent optional and
defaulted controls for every enclosing scope and endpoint value. They infer no required scope value
from client state. Facade-created and explicit endpoints produce identical method, logical/safe URL,
query order, public headers, timeout, body metadata, response behavior, and ownership-constrained
execution. Cross-client endpoints, arbitrary values, endpoint capability forgery, and extension
trait forgery fail at compile time.

Direct await works after endpoint extension setters. Buffered, Bytes, and NoContent requests retain
`.response()`; response streams remain direct-only and do not gain the metadata terminal.

## Request shaping and lifecycle

The one `HeadBuilder` begins from the parsed fixed base URL. Host labels prefix left-to-right and
must be non-empty ASCII DNS labels of at most 63 bytes with no underscore, dot, port, Unicode,
leading/trailing hyphen, slash, or backslash. Fixed scheme, port, and base path are preserved.

Normalized path atoms append after the fixed base path with exactly one separator. Each atom is one
segment; empty/dot segments, slash/backslash injection, and traversal fail. Percent input is encoded
as data exactly once. Query shaping uses one ordered pair sequence, preserves unrelated inherited
order and vector element order, supports scalar/`Option`/`Vec`/`Option<Vec>`, emits repeated pairs,
and applies replacement/removal/off before code generation. URL serialization owns encoding.

Public headers enter the existing case-insensitive `PreparedHead` map. Invalid/reserved names and
invalid values fail before hooks. Scalar and optional values are supported; vectors fail during
semantic analysis. Public `Accept` wins over the response default. Codec-, stream-, and
multipart-owned `Content-Type` conflicts fail; an explicit public `Content-Type` is allowed only
when no selected body source owns it. Multipart leaves top-level boundary ownership to Reqwest.

Safe URL rendering occurs after host/path/public-query shaping and before hooks. Sensitive public
query values are redacted while successful logical URLs retain ordinary public values. Logical and
safe URLs are immutable after preparation. No authentication query insertion exists in Phase 5.

Client/scope/endpoint timeout declarations normalize to inherit/set/off; request-local `timeout`,
`disable_timeout`, and `inherit_timeout` remain distinct. Duration syntax rejects zero where
required and checked overflow. First poll allocates identity/deadline, formats the normalized public
expressions, validates head/body metadata/media ownership, and only then invokes `before_send`.
Invalid shaping invokes no hook, body codec/factory/stream, native request construction, or HTTP.
The established body materialization, managed Reqwest execution, response handling, completion, and
typed terminal paths are unchanged.

## Methods, aliases, and names

The compiler supports `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `TRACE`,
`CONNECT`, and validated `METHOD("TOKEN")`. Invalid custom tokens fail at the declaration span.
Endpoint aliases select the public method/name while retaining deterministic internal identity.
Rust keywords are escaped deterministically.

Separate semantic namespaces detect normalized collisions among clients/support modules, child
facades/endpoints, aliases, explicit endpoint types/modules, builder/facade/request setters,
clear/reset helpers, generic request methods, terminals, endpoint traits, and prelude aliases.
Multiple client preludes and same-named endpoint setters coexist because trait aliases contain the
client and full lexical endpoint path; typed receivers disambiguate methods.

## Generated API and Rustdoc

Every client gets one public support module with `facades`, `endpoints`, and `prelude`; credential
modules remain a later-phase concern. Root endpoints and nested facade/endpoint modules compile.
Normal and renamed runtime dependencies, nested macro invocations, repeated invocations, multiple
clients, and generated contract mismatch are maintained fixtures.

Rendered generated Rustdoc documents required/optional/default values, constructor/builder/facade
signatures, endpoint request extension import, methods, route shape, public query/header categories,
timeouts/retry, body/response modes, features, and explicit endpoint usage. It contains no resolved
runtime values, native URL, header/query/body values, private snapshot, seal, normalized IR,
Reqwest object, core endpoint-map bridge, or `concord_core::__private` path. The Rustdoc fixture
builds with a renamed runtime dependency and nested generated APIs.

## Feature behavior and regressions

- Default and all-feature configurations use the same generated and runtime path.
- `--no-default-features` retains managed HTTP and passes the maintained no-default gate.
- JSON remains independently feature-owned and uses the buffered codec path.
- Multipart remains request-only, independently feature-gated, Reqwest-framed, and managed.
- Text, Bytes, NoContent, custom buffered codecs, request streams, response streams, body factories,
  terminal status bodies, endpoint ownership, and media behavior regress through `just release`.
- `dangerous-custom-retry` is independent and its generated DSL has focused enabled/disabled proof.
- `dangerous-raw-response` remains topology-only. No positive raw successful-response API is
  implemented; Phase 10 owns it.
- Macro/runtime dependency separation and the no-default dependency graph remain enforced.

## Tests and validation

Focused proof before the full gate included:

| Exact command | Result | Count |
|---|---|---:|
| `cargo nextest run --profile ci -p concord_core -p concord_macros --all-features` | pass | 368/368 |
| `cargo nextest run --profile ci -p concord_macros --all-features` | pass | 61/61 |
| `cargo nextest run --profile ci -p concord_macros --all-features --test phase5_generated` | pass | 10/10 |

These suites include parser boundaries, semantic resolution, normalized IR, deterministic naming,
Trybuild/UI diagnostics, missing/available prelude import, trait sealing and endpoint specificity,
multi-client preludes, client/scope/endpoint default counts, immutable snapshots, explicit/facade
parity, all standard/custom methods, exact local Reqwest paths/query/header behavior, timeout
inheritance, safe URL redaction, generated Rustdoc, renamed dependencies, generated contract
mismatch, and existing body/response regressions.

The complete maintained matrix against `c513710c1e6ae2333a58ec481c2b74a4a0529a5f` is:

| Exact command | Result | Count / note |
|---|---|---|
| `just fmt-check` | pass | formatting clean |
| `just check` | pass | default workspace check |
| `just check-all` | pass | all-feature workspace check |
| `just check-no-default` | pass | maintained no-default check |
| `just clippy` | pass | warnings denied |
| `just clippy-all` | pass | all features, warnings denied |
| `just test` | pass | 367/367 |
| `just test-all` | pass | 371/371 |
| `just test-no-default` | pass | 1/1; 214 filtered/skipped by maintained recipe |
| `just doctest` | pass | 2/2 |
| `just doctest-all` | pass | 2/2 |
| `just doc` | pass | three library Rustdoc builds, warnings denied |
| `just doc-all` | pass | all-feature Rustdoc, warnings denied |
| `just deny` | pass | accepted duplicate `core-foundation`/`windows-sys` warnings only |
| `just diff-check` | pass | no whitespace errors |
| `just release` | pass | complete gate in 65.9 seconds |

`just release` repeated the default 367, all-feature 371, no-default 1, doctest, documentation,
deny, and diff gates. Its maintained dangerous matrix passed default 2/2, custom retry 3/3,
development tools 2/2, and raw-response flag 2/2.

During integrated review, stale path tests that expected slash encoding were corrected to the
normative rejection contract and percent-data exactly-once case. The development-feature lock was
regenerated after macro `http`/`url` dependencies changed. Clippy identified an async mutex issue,
which was corrected. A later semantic review found defaulted outer `Option` values incorrectly
classified as optional-only; default selection now takes precedence while query shape remains
`Option`/`Option<Vec>`. Every affected focused and maintained command was rerun and passed. No
validation was unavailable and no weaker command was substituted.

## Evidence and coverage

The catalog adds 198 Phase 5 source-behavior rows: 143 direct and 55 partial. Four direct and one
partial catalog/report-integrity rows bring Phase 5 to 203 rows: 147 direct and 56 partial. The
complete catalog has 771 rows and 771 unique evidence IDs. All IDs refer to one of the 504 exact
normative requirement IDs; unknown IDs are zero.

All 568 Phase 0-4 evidence rows, including all 445 Phase 0-3 rows, are byte-identical to the Phase 4
anchor. All 198 source rows point to artifacts that exist at the immutable tested source commit.
The five audit artifacts exist at the complete evidence-state commit and are reanchored later to
that state without adding IDs or changing counts/classifications.

The 504-row coverage plan retains `PLAN-*` IDs and `planned` status so planning remains distinct
from actual passing evidence. Exactly 198 reviewed rows now point to Phase 5 source/tests and exact
`P5-*` evidence IDs in their notes. In particular:

- `API-CODEC-003`, `GEN-EXPL-002`, and `GEN-NAME-002` are direct;
- `EXEC-HEAD-001` is partial only for Phase 6 authentication collision/TLS clauses;
- `CONF-SEMA-001` is partial only for later authentication/rate/pagination IR.

Both CSVs are LF-only. Schema/count/uniqueness/unknown-ID/history/artifact checks pass.

## Source and architecture audit

The final source has one compiler pipeline, one `Request<E>` pending wrapper, one `HeadBuilder`, one
`PreparedHead`, one public-header map, one body architecture, one private managed Reqwest executor,
and one response pipeline. Facades and explicit endpoints construct the same endpoint type and use
the same request path. The endpoint-map bridge modifies only unpolled endpoint state.

There is no runtime API tree, second query/header production representation, generated request
wrapper, facade-only or explicit-only executor, public transport/executor generic, raw Reqwest
request/client/body, runtime `Any`, credentials, authentication, provider integration, limiter,
pagination execution, prepared public endpoint, or positive raw successful-response terminal.

## Remaining partial requirements

Every partial Phase 5 requirement is listed below. “Proved” identifies the exact Phase 5 portion;
“missing” identifies the later normative portion and its owner.

| Exact requirement IDs | Proved in Phase 5 | Missing portion / owner |
|---|---|---|
| `PROD-GUAR-001`, `DSL-QUERY-003`, `SEC-URL-001`, `SEC-URL-003`, `SEC-QUERY-001` | Ordered public query, logical URL, safe redaction, and no auth insertion | Implemented managed query authentication and auth-key redaction — Phase 6 |
| `PROD-GUAR-003`, `GEN-TYPE-002`, `GEN-CTOR-002`, `GEN-BUILD-002`, `GEN-CLONE-001`, `GEN-CLONE-003`, `SEC-SECRET-001`, `SEC-SECRET-002` | Private current snapshots, public-value ordering/replacement/cloning, and impossible credential expression flow | Real secret/provider arguments, storage, clone state, formatting, and flow proof — Phase 6 |
| `GEN-BUILD-001` | One canonical builder for all Phase 5 value/HTTP/runtime/retry inputs | Credential/provider and limiter builder inputs — Phases 6-7 |
| `GEN-DOC-002` | Rustdoc excludes current private integration and runtime values | Credential, rate-key, cursor, and later generated exclusions — Phases 6-8 |
| `PROD-GUAR-002`, `SEC-TEST-001` | Current errors/hooks/debug/completion/wrappers are body-free and sentinel-tested | Provider, limiter, pagination, and credential diagnostic matrices — Phases 6-8 |
| `PROD-GUAR-010`, `SEC-FAIL-001` | Route/header/media/TLS/body-limit failures fail closed | Auth collision, limiter enforcement/key, and pagination-cycle failures — Phases 6-8 |
| `API-PRIVATE-001`, `API-PRIVATE-002` | Current generated extension needs no private import and exposes no prohibited native/runtime surfaces | Later auth/rate/page/prepared/raw surfaces and final integration audit — Phases 6-10 |
| `DSL-TIME-002` | Endpoint timeout inheritance and request-local overrides | Independent page-local deadline — Phase 8 |
| `BOUND-MATH-001`, `BOUND-MATH-002` | Checked duration, length, route-limit, and current count arithmetic | Limiter quota and pagination arithmetic/counters — Phases 7-8 |
| `EXEC-BUILD-002`, `EXEC-CONSTRUCT-001`, `EXEC-CREATE-001`, `EXEC-CREATE-002` | Current construction/request creation is network-, hook-, body-, shaping-, and execution-free as specified | Provider, limiter, and pagination-controller exclusions — Phases 6-8 |
| `EXEC-ORDER-001`, `EXEC-HEAD-001`, `EXEC-HOOK-001`, `EXEC-PREC-001` | Public head/body metadata/hook/native/response order and earliest current failure | Auth/provider, limiter, recovery, and pagination phases — Phases 6-8 |
| `EXEC-NATIVE-002`, `EXEC-DEADLINE-001`, `EXEC-DEADLINE-004`, `EXEC-CANCEL-001`, `EXEC-COUNT-001`, `EXEC-COUNT-002` | Current retry invisibility, deadlines, cancellation, and attempt behavior | Provider/auth recovery, limiter coupling, and page attempts — Phases 6-8 |
| `EXEC-COMPLETE-001`, `EXEC-COMPLETE-005` | One correlated outside-deadline direct-request completion | Page and aggregate pagination completion metadata — Phase 8 |
| `FEAT-DANGER-001` | Independent custom-retry/development/raw-flag topology | Positive raw successful-response API — Phase 10 |
| `COMPAT-SURF-001` | Public generated prelude/extension-trait sufficiency | Prepared endpoints and complete extension certification — Phase 9 |
| `COMPAT-BUILD-001` | Rust 1.85 proof for the Phase 5 product | Release-final completed-product platform matrix — Phase 10 |
| `CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`, `CONF-DOWNSTREAM-001`, `CONF-DOWNSTREAM-002` | Current generated/advanced/import/diagnostic workflows | Later generated categories and complete extension certification — Phases 6-10, especially Phase 9 |
| `CONF-PARSER-001`, `CONF-SEMA-001` | Complete Phase 5 grammar/prohibition and normalized inherited IR proof | Auth/rate/page and release-final grammar/IR coverage — Phases 6-8 and 10 |
| `CONF-ORDER-001`, `CONF-SEC-001` | Current deterministic side-effect and source-chain/security matrices | Provider/auth, limiter, pagination, and release-final matrices — Phases 6-10 |
| `CONF-TRACE-006` | Every Phase 5 row has exact immutable artifact ownership | Actual release evidence for every applicable normative requirement — Phase 10 |
| `CONF-MSRV-001`, `CONF-DOC-001` | Rust 1.85 and compiled canonical documentation for all current APIs | Completed-product MSRV/examples/documentation — Phase 10 |

The table contains all 55 unique partial source requirements. `P5-AUDIT-005` is the second partial
row for `CONF-TRACE-006`, so the evidence catalog contains 56 partial Phase 5 rows.

## Phase 6 readiness

Phase 5 leaves the immutable client/scope/endpoint snapshots, normalized inherited IR, first-poll
protected head preparation, body replayability metadata, operation identity/deadline, and one
executor ready for authentication integration. Phase 6 may add credential/auth behavior only
through this established path. It must not add a request wrapper, runtime API tree, head builder,
public-header map, transport, or executor.

`PHASE_6_NOT_STARTED`
