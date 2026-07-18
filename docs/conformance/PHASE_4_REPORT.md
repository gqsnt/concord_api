# Phase 4 Integrated Conformance Report

## Verdict

`PHASE_4_COMPLETE_WITH_LIMITATIONS`

All Phase 4 exit criteria are satisfied. The limitations in this report belong to broader 0.1
requirements whose remaining clauses are owned by Phases 5-10; none invalidates a Phase 4 exit
criterion. Phase 5 has not started.

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable evidence identity

- Starting HEAD and final Phase 3 anchor:
  `f5cd7e98ade6474f2893ae9199884fb05d651004`
- Branch: `master`
- Initial working tree: clean
- Final Phase 3 source:
  `54a64cb4ba062453dea26dd6af1dff97eaa3406e`
- Final Phase 3 evidence/report:
  `ef8df96db9ac6594998327fa108af9f25ba787f7`
- Final Phase 3 anchor:
  `f5cd7e98ade6474f2893ae9199884fb05d651004`
- Immutable tested Phase 4 source:
  `6472bc01e55bbd4282a8d5e06deeb9126650f9c4`
- Final Phase 4 validation/evidence time: `2026-07-18T01:06:38Z`
- Phase 4 evidence/report commit: named by the following documentation-only anchor
- Phase 4 evidence anchor: this report's documentation-only follow-up
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Cargo resolver: 3
- Nextest exercised: 0.9.140; repository minimum 0.9.115
- Managed HTTP baseline: Reqwest 0.13.4 with default features disabled

The source tree was clean when final validation began and remained unchanged throughout the
maintained and focused final matrices. All 118 source-behavior rows point to the immutable source
commit. Catalog/report-integrity rows are anchored separately to the evidence/report commit so
every cited artifact exists at its recorded owner.

## Phase 4 commits

| Unit | Commit | Subject | Durable result |
|---|---|---|---|
| 4.0 | `b59de71566ffbfdad9da839e6781badeb2fe18f1` | `test: maintain dangerous namespace release proof` | Added the feature-specific dangerous namespace UI proof to `just release` without enabling development tools. |
| 4.1-4.2 | `8b2d9dc1efbce0e50215a492854ee79fc6ac09f2` | `feat: add managed request streams and body factories` | Added safe body contracts, instance metadata, one prepared-body model, request streams, factories, internal replayability, exact-length/limit accounting, cancellation, and Reqwest probes. |
| 4.3 | `eed624947af12cbdc7897529fb8c8b45fc4b12b9` | `feat: add bounded lazy response streams` | Added the typed lazy stream response handler/wrapper, media validation, finite cumulative limits, deadline boundary, and `StreamOpened`. |
| 4.4 | `d1967fc0afef27f33578749c39faafd584e6fa38` | `feat: add bounded request multipart` | Added feature-gated multipart recipes/factories, Reqwest-owned framing, aggregate accounting, replayability, redaction, and local upload proof. |
| 4.5 | `6472bc01e55bbd4282a8d5e06deeb9126650f9c4` | `feat: generate stream and multipart endpoints` | Added the one parser/model/codegen vertical slice, diagnostics, terminal boundaries, renamed/nested/repeated compatibility, generated runtime, and Rustdoc proof; advanced the macro/core contract to version 5. |

Each source unit received focused validation, an uncommitted full patch review, a committed patch
capture under ignored `git_diff/`, and a coherent commit before the next unit began.

## Contract map and authority review

The complete pre-source map is the ignored artifact `git_diff/phase4-contract-map.md`. It was
finished before product source changes and is intentionally not committed. It records the verified
baseline, exact public signatures, 118 exact requirement IDs, source migration, factory
reachability, contradiction decisions, test oracles, and parallel-architecture prevention.

The exact inventory is:

- product: `PROD-CORE-002`, `PROD-CORE-003`, `PROD-GUAR-002`, `PROD-GUAR-004`,
  `PROD-GUAR-008`, `PROD-GUAR-010`, `PROD-STATE-003`, `PROD-SURF-002`;
- DSL/generated: `DSL-PARAM-001`, `DSL-END-003`, `DSL-BODY-002`, `DSL-RESP-002`,
  `DSL-PAGE-001`, `DSL-FEAT-002`, `GEN-END-001`, `GEN-END-002`, `GEN-END-003`,
  `GEN-TERM-001`, `GEN-TERM-002`, `GEN-TERM-003`, `GEN-EXPL-001`, `GEN-EXPL-002`,
  `GEN-OWN-001`, `GEN-OWN-002`, `GEN-DOC-001`, `GEN-DOC-002`;
- public API: `API-NS-001`, `API-REQ-001`, `API-REQ-002`, `API-CODEC-004`,
  `API-BODY-001`, `API-BODY-002`, `API-BODY-003`, `API-RESP-001`, `API-HOOK-004`,
  `API-RUNTIME-001`, `API-PRIVATE-001`, `API-PRIVATE-002`;
- security/bounds: `SEC-THREAT-001`, `SEC-BODY-001`, `SEC-BODY-002`, `SEC-RESP-002`,
  `SEC-RESP-003`, `SEC-RESP-005`, `SEC-HOOK-001`, `SEC-FAIL-001`, `SEC-EXT-001`,
  `SEC-TEST-001`, `BOUND-BODY-001`, `BOUND-BODY-003`, `BOUND-BODY-006`,
  `BOUND-BODY-007`, `BOUND-BODY-011`;
- execution: `EXEC-CREATE-001`, `EXEC-CREATE-002`, `EXEC-HEAD-001`,
  `EXEC-BODYMETA-001`, `EXEC-HOOK-001`, `EXEC-HOOK-002`, `EXEC-BODY-001`,
  `EXEC-BODY-003`, `EXEC-BODY-004`, `EXEC-BODY-005`, `EXEC-NATIVE-001`,
  `EXEC-NATIVE-002`, `EXEC-RESPHEAD-001`, `EXEC-RESPHEAD-002`, `EXEC-PREC-001`,
  `EXEC-BUFFER-003`, `EXEC-NOCONTENT-001`, `EXEC-STREAM-001`, `EXEC-STREAM-002`,
  `EXEC-STREAM-003`, `EXEC-DEADLINE-001`, `EXEC-DEADLINE-005`, `EXEC-COMPLETE-001`,
  `EXEC-COMPLETE-003`, `EXEC-COMPLETE-005`, `EXEC-CANCEL-001`, `EXEC-ORDER-001`;
- feature/compatibility: `FEAT-CORE-001`, `FEAT-CORE-002`, `FEAT-NODEFAULT-001`,
  `FEAT-MULTIPART-001`, `FEAT-DIAG-001`, `FEAT-DANGER-001`, `FEAT-DANGER-002`,
  `COMPAT-CRATE-001`, `COMPAT-RUST-001`, `COMPAT-RUST-002`, `COMPAT-REQWEST-001`,
  `COMPAT-REQWEST-003`, `COMPAT-MACRO-001`, `COMPAT-MACRO-002`,
  `COMPAT-MACRO-003`, `COMPAT-GEN-001`, `COMPAT-GEN-002`, `COMPAT-SURF-001`,
  `COMPAT-BUILD-001`, `COMPAT-DEP-001`, `COMPAT-DEPS-001`;
- conformance: `CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`,
  `CONF-DOWNSTREAM-001`, `CONF-DOWNSTREAM-002`, `CONF-DET-001`,
  `CONF-REQWEST-001`, `CONF-ORDER-001`, `CONF-SEC-001`, `CONF-MSRV-001`,
  `CONF-DOC-001`, `CONF-TEST-001`, `CONF-CI-001`, `CONF-CI-002`, `CONF-CI-003`,
  `CONF-CI-004`, `CONF-VERTICAL-001`.

No strategic contradiction required `NEEDS_DECISION`. The resolutions were:

- evolve the existing static body contract into borrowed instance metadata rather than add a
  second trait;
- keep every direct `StreamBody`, including `from_bytes`, one-shot while retaining trustworthy
  exact/lower-bound metadata;
- materialize factories only after hook approval, once per current Concord attempt;
- store reconstructible factory ownership for Phase 6 without implementing recovery;
- retain the managed body error before reducing a less-specific Reqwest transport failure;
- have Reqwest create multipart boundaries and serialization, then account every emitted byte in
  the same managed native-body adapter;
- validate stream media and known length before returning the lazy body;
- complete `StreamOpened` once and leave later polling outside the endpoint deadline;
- omit `MetadataResponse` from stream handlers so `.response()` is absent statically;
- keep pagination, multipart response mode, factory DSL markers, and prepared public endpoints
  absent.

Factory reachability is resolved without inventing a public route: `RequestBody` implements the
same private `BodySpec`, and internal endpoint tests execute it through the production executor.
Downstream tests prove public `BodyFactory` imports. Phase 9 still owns prepared public endpoints;
Phase 6 owns an actual reconstructed authentication-recovery attempt.

## Phase 3 follow-up

`just release` now depends on the private `_test-ui-dangerous` recipe:

```text
cargo nextest run --profile ci -p concord_core --features dangerous-custom-retry --test ui_dangerous
```

It enables `dangerous-custom-retry`, does not enable `dangerous-dev-tools`, uses ordinary Trybuild,
and has no nested Cargo fixture. The maintained proof passed 1/1 both independently and within
`just release`. Default and all-feature behavior remain unchanged. No new normative row was added
solely for moving the already-valid proof into the release matrix.

## Public body and stream surface

`concord_core::advanced` exports:

- `BodyError`, `BodyErrorKind`, and `BodyKind`;
- `BodyFactory` and `RequestBody`;
- `MediaType`, `OctetStream`, and `StreamBody`;
- `StreamResponse<M>`;
- under `multipart` only, `MultipartBody`, `MultipartPart`, and `MultipartFactory`.

The prelude exports `StreamBody`, `StreamResponse<M>`, and the retained `OctetStream`. The crate
root retains `Request<E>` and `ResponseRequest<E>`. Multipart recipe/factory types are advanced-only
and absent without the feature. No public signature exposes Reqwest bodies, body-spec variants,
replayability, generated-private paths, or a transport/executor.

`BodyError` has private fields, structural kinds, trusted safe-message construction, body-free
`Display`/`Debug`, and no unsafe producer source text. The trusted downstream constructor is
documented as application-authored safe text. `RequestBody` provides empty, bytes, stream, factory,
multipart, multipart-factory, content-type, and body-kind operations without a public replayability
switch.

## One request-body architecture

The existing sealed `BodySpec` now performs side-effect-light `&self` metadata inspection.
`BodyMetadata` carries body kind, private replayability, exact length/lower-bound facts, and
validated content-type ownership without encoding, polling, opening files, or calling factories.

One private `PreparedBody` represents empty, reusable bytes, one-shot streams, stream factories,
replayable/one-shot multipart, and multipart factories. Empty and bytes are reusable; direct
streams are one-shot; factories are reconstructible; multipart derives from all parts. Application
code cannot label or query replayability.

One `ManagedRequestStream` is the native streaming boundary. It uses checked arithmetic, enforces
the request limit and exact length, withholds the first frame crossing either bound, detects
underflow, records a safe primary body error, and stops polling when dropped. Buffered codecs,
direct streams, factories, and Reqwest-generated multipart all converge on this model and the
existing `ApplicationHttp` execution boundary.

## Request streams and factories

`StreamBody` implements the exact public constructors `from_bytes`, `from_stream`, `from_reader`,
`from_file`, and `exact_length`. `futures-core` and `tokio-util` are direct dependencies where their
public/implementation contracts require them; macro normal dependencies remain runtime-free.

Deterministic tests prove zero stream polls during construction, request creation, metadata,
first-poll public-head preparation, hook setup, and veto. Exact success, underflow, overflow,
cross-frame overflow, aggregate-limit conflict, impossible known size, producer failure precedence,
first-excess withholding, file/reader behavior, and cancellation are covered. Test-owned loopback
HTTP proves exact transmitted bytes and content type.

`BodyFactory::content_type()` is inspected without `create()`. Factory `create()` occurs exactly
once after hook approval for the current Concord attempt; veto and factory failure cause zero
native calls. Reqwest receives the produced body, so a hidden native retry cannot reinvoke the
factory. A retained factory can explicitly construct another attempt body through the same private
model, but no Phase 4 production path initiates recovery. The authentication-recovery clause
therefore remains partial until Phase 6.

## Response streams

`StreamResponse<M>` owns the remaining native response body and exposes logical URL, status,
version, actual successful headers, content length, and execution metadata. It implements
`Stream<Item = Result<Bytes, ApiError>>` without requiring `M: Debug`.

The existing response-handler architecture performs finite known-length preflight, validates the
one `MediaType` contract, and constructs the lazy stream. Unknown bodies remain unpolled before
return. The existing positive finite `stream_response_limit` defaults to 16 MiB and is independent
of the buffered-response limit. Cumulative post-decompression bytes use checked arithmetic and the
first excess chunk is withheld. Native failures become safe body-free `ApiError` values; yielded
chunks are not rolled back.

The endpoint deadline covers native request, response head, status, known-length/media validation,
and successful stream construction. It is not stored in the returned stream. The fixed
stream-open result completes once with `CompletionOutcome::StreamOpened` outside the deadline;
later failure, limit, partial consumption, or drop emits no second completion. `Debug` is
non-consuming and omits chunks, header values, and unsafe URL details.

## Multipart

The `multipart` core feature activates only `reqwest/multipart`. It does not activate JSON,
compression, or dangerous features. Without it, multipart public types are absent and generated
syntax produces the focused `concord_core/multipart` diagnostic; no fallback body mode is chosen.

`MultipartBody` and `MultipartPart` support validated text, bytes, streams, filenames, part media
types, and fallible insertion. `MultipartFactory` produces a fresh validated recipe. Text/bytes
parts are reusable, direct stream parts are one-shot, direct recipe replayability derives from all
parts, and multipart factories are reconstructible. Top-level content-type override is rejected.

Reqwest owns form serialization, boundary generation, and top-level `Content-Type`. Concord does
not serialize multipart syntax. It wraps the resulting Reqwest body in `ManagedRequestStream`, so
boundaries, headers, framing, and payload all count toward the aggregate request limit. A focused
test proves payload bytes can fit while total framing exceeds the limit. Local HTTP proves actual
framing, streamed parts, boundary-bearing content type, and cancellation. Errors, sources, hooks,
completion, Debug, and Rustdoc exclude multipart values, filenames, chunks, unsafe producer text,
and diagnostic boundary disclosure.

## Generated DSL and API

The existing parser/model/codegen pipeline now normalizes:

```text
body: Stream<M>  -> StreamBody
body: Multipart  -> MultipartBody
-> Stream<M>     -> StreamResponse<M>
```

The parser preserves one body, body-last ordering, parentheses, and the current GET/POST subset.
It rejects malformed/nested stream markers, multipart responses, multipart without its feature,
and pagination on stream/bytes/no-content responses without implementing pagination. No factory
marker or multipart response marker exists.

Generated facade and explicit endpoint constructors share the same owned values, sealed
API-specific capability, body types, response handlers, and executor. Direct stream await returns
`StreamResponse<M>`; the stream handler does not implement the sealed metadata terminal, so
`.response()` fails at compile time. No pagination or raw terminal is generated. No unnecessary
`Clone` bound is added to streams, media markers, multipart values, or producers.

Generated runtime tests execute ordinary and custom-media uploads/downloads plus multipart over
test-owned local HTTP, for facade and explicit endpoints, while retaining Text, JSON, custom codec,
Bytes, and NoContent regressions. Compile tests prove renamed dependencies, missing features,
contract mismatch, nested/repeated expansions, multiple APIs, cross-client rejection, arbitrary
value rejection, and downstream capability sealing. Contract marker version 5 is advanced
atomically in core and macro. Rendered Rustdoc describes the public modes/limits/ordering and scans
out private body, stream, replayability, Reqwest, and generated integration names.

## Execution lifecycle and cancellation

The single production path is:

```text
DSL and generated/explicit/internal endpoint
-> typed Request<E>
-> first-poll OperationId and deadline
-> public head and instance body metadata
-> before_send
-> one post-hook materialization/current-attempt body
-> one managed native-body boundary
-> existing managed ApplicationHttp/Reqwest execution
-> response-head debug and hook
-> status classification
-> bounded buffered value, unpolled NoContent, or lazy StreamResponse<M>
-> fixed result and outside-deadline completion
-> direct semantic result
-> later caller stream polling outside the endpoint operation
```

Cancellation is deterministic during a pending hook, direct/factory request stream, multipart
stream part, and response stream. Drop prevents later polling. Endpoint completion retains the
existing allowance that cancellation or panic can prevent delivery; returning a stream completes
only the open event and never its later consumption.

## Feature and dependency behavior

- default: request/response streams work; 336/336 tests pass;
- all features: streams, multipart, JSON, compression wiring, and dangerous surfaces coexist;
  340/340 tests pass;
- no default: core checks pass, managed HTTP proof passes, public body/stream fixtures pass 3/3,
  and focused stream execution passes 10/10;
- multipart enabled: 11/11 focused tests and multipart-only all-target check pass;
- multipart disabled: types are absent and both core/macro focused diagnostics pass;
- JSON remains independently owned and is not activated by multipart;
- dangerous custom retry remains independently selectable and does not activate development tools;
- Reqwest remains mandatory; `concord_macros` has no runtime-only normal dependency;
- Rust 1.85.0, edition 2024, resolver 3, and Reqwest 0.13.4 are unchanged.

## Validation results

### Maintained commands

| Exact command | Result | Count/detail |
|---|---|---|
| `just fmt-check` | pass | all workspace formatting unchanged |
| `just check` | pass | workspace all targets |
| `just check-all` | pass | workspace all targets/all features |
| `just check-no-default` | pass | core all targets/no defaults |
| `just clippy` | pass | workspace all targets, warnings denied |
| `just clippy-all` | pass | workspace all targets/all features, warnings denied |
| `just test` | pass | 336/336 Nextest tests |
| `just test-all` | pass | 340/340 Nextest CI-profile tests |
| `just test-no-default` | pass | 1/1 maintained no-default managed HTTP test; 207 filtered |
| `just doctest` | pass | 2/2 core compile-fail docs; macro/test-support have zero |
| `just doctest-all` | pass | 2/2 core compile-fail docs; macro/test-support have zero |
| `just doc` | pass | core, macro, and test-support Rustdoc with warnings denied |
| `just doc-all` | pass | all-feature Rustdoc for all three libraries |
| `just deny` | pass | advisories, bans, licenses, and sources; only tolerated duplicate-version warnings for `core-foundation` and `windows-sys` |
| `just diff-check` | pass | no whitespace errors |
| `just release` | pass | default CI 336/336; all-feature CI 340/340; no-default 1/1; dangerous UI 1/1; doctest/doc/deny/diff gates pass |

### Focused Phase 4 commands

| Exact command/proof | Result |
|---|---|
| `cargo nextest run -p concord_core -E 'test(/body::stream::tests::/) \| test(/body::prepared::tests::(factory\|managed_stream\|request_body)/) \| test(/execute::tests::(body_factory\|bytes_reader_and_file_stream\|cancellation_drops_direct\|managed_producer\|managed_request_stream\|producer_failure\|real_reqwest_stream_upload\|request_stream_and_factory)/)'` | 14/14 pass |
| `cargo nextest run -p concord_core --all-features -E 'test(/response::stream::tests::/) \| test(/execute::tests::(caller_stream\|response_stream\|stream_response\|unknown_stream)/) \| test(=stream_response_api::downstream_stream_response_has_the_exact_public_stream_and_metadata_surface)'` | 8/8 pass, including post-decompression and deadline boundary |
| `cargo nextest run -p concord_core --features multipart -E 'test(/multipart/)'` | 11/11 pass |
| `cargo check -p concord_core --no-default-features --features multipart --all-targets` | pass |
| `cargo nextest run -p concord_core --no-default-features -E 'test(/body::stream::tests::/) \| test(/execute::tests::(real_reqwest_stream_upload\|request_stream_and_factory\|stream_response\|unknown_stream\|caller_stream)/) \| test(/response::stream::tests::/) \| test(/body_extensions/) \| test(/stream_response_api/)'` | 10/10 pass |
| `cargo nextest run -p concord_core --no-default-features --test body_extensions --test stream_response_api` | 3/3 downstream public API tests pass |
| `cargo nextest run -p concord_core --all-features --test reqwest_contract -E 'test(/(stream|multipart|hidden_retry)/)'` | 5/5 Reqwest probes pass |
| `cargo nextest run -p concord_core --all-features --test ui --test public_rustdoc` | 2/2 core UI/Rustdoc harnesses pass |
| `cargo nextest run -p concord_macros --test ui --test compile --test generated_surface` | 24/24 UI, fixture, Rustdoc, and generated runtime tests pass |
| `cargo nextest run -p concord_core --all-features -E 'test(/(validation_and_debug_are_structural_and_redacted\|producer_failure\|multipart_stream_failure\|debug_is_non_consuming\|cancellation_drops_direct\|caller_stream_polling)/)'` | 9/9 pass |
| `cargo nextest run --profile ci -p concord_core --features dangerous-custom-retry --test ui_dangerous` | 1/1 pass |
| `cargo nextest run -p concord_macros --lib` | 12/12 parser/codegen unit tests pass |

### Corrections and unavailable evidence

Focused implementation validation found and corrected: one Rust 1.85-incompatible let-chain in the
parser; one incomplete generated request-stream Rustdoc sentence; fixture workspace/locked-lockfile
registration; one overly broad multipart Rustdoc assertion; and one attempted command that named a
nonexistent `parser` integration target (rerun correctly as `cargo nextest run -p concord_macros
--lib`). Every applicable corrected command then passed.

The additional, non-maintained command
`cargo nextest run -p concord_core --no-default-features` is not a Phase 4 gate and remains
unavailable as a full-suite proof: Nextest stopped at 132/276 after 122 passes and 10 failures (144
not run). All ten failures are pre-existing TLS/development HTTP unit tests that build a
`rustls-no-provider` Reqwest client without installing a process crypto provider. The maintained
no-default recipe intentionally owns the provider-independent proof. Phase 4 no-default source,
all-target compilation, public stream contracts, and focused real stream execution all pass. This
does not invalidate a Phase 4 criterion and no weaker command was substituted for a maintained
gate.

## Self-review and architecture audit

Complete uncommitted and committed patches were inspected for every unit. Corrections were made
before each commit and affected commands rerun. The final audit confirms:

- one `Request<E>` future and `execute_request` production executor;
- one sealed `BodySpec`, one `BodyMetadata`, and one `PreparedBody` model;
- one `ManagedRequestStream` native accounting/error boundary;
- one existing managed `ApplicationHttp`/Reqwest executor;
- one response-handler architecture and one lazy `StreamResponse<M>` wrapper;
- Reqwest is the only multipart serializer;
- no public replayability flag/query, transport, executor, or native request/client/body;
- no runtime `Any`, second response loop, manual multipart syntax, or factory-only proof model;
- no authentication/recovery, limiter execution, pagination execution, prepared public endpoint,
  or Phase 5 request shaping was added.

The only public Reqwest re-export remains the pre-existing feature-gated dangerous retry builder,
which is outside ordinary body/execution surfaces and unchanged by Phase 4.

## Evidence and coverage

The 118 exact Phase 4 source-behavior rows comprise 73 direct and 45 partial classifications. The
documentation-only anchor adds five catalog/report-integrity rows (four direct and one partial), so
the final Phase 4 addition is 123 rows: 77 direct and 46 partial. The final catalog contains 568
rows and 568 unique evidence IDs. The coverage plan retains 504 rows and 504 unique normative IDs.
There are zero unknown requirement IDs in either catalog.

All 445 historical evidence rows are unchanged. Coverage edits affect only the 118 reviewed Phase
4 requirement rows, retain their planning identifiers/status, name current exact source paths and
evidence IDs, and distinguish direct proof from deferred clauses. Source rows are owned by
`6472bc01e55bbd4282a8d5e06deeb9126650f9c4`; the five audit rows are owned by the later
evidence/report commit. The final anchor runs `git cat-file -e <verified_commit>:<path>` for every
listed Phase 4 artifact and reviews all classifications and commands.

## Remaining partial requirements

Every partial source requirement has an explicit proved portion and future owner:

| Requirement IDs | Proved in Phase 4 | Missing portion and owner |
|---|---|---|
| `PROD-CORE-003` | generated, explicit, and internal `RequestBody` endpoints share one runtime | prepared public endpoints — Phase 9 |
| `PROD-GUAR-002` | stream/multipart body-free automatic diagnostics | provider, limiter, pagination diagnostic categories — Phases 6-8 |
| `PROD-GUAR-004` | bounded application request/buffered/stream response I/O | provider HTTP body execution — Phase 6 |
| `PROD-GUAR-008` | hidden retry does not rerun hooks/factories | credentials, limiter, pagination preparation — Phases 6-8 |
| `PROD-GUAR-010` | body/media/limit failures fail closed | auth placement, limiter availability, pagination cycles — Phases 6-8 |
| `GEN-EXPL-002` | required enclosing/endpoint/body order | optional/defaulted scope parity — Phase 5 |
| `GEN-DOC-002` | body, filename, native URL, private storage exclusion | credential, rate-key, cursor, release-wide categories — Phases 6-10 |
| `API-PRIVATE-001`, `COMPAT-SURF-001` | current body/media/factory workflows require only public imports | prepared endpoints and complete extension certification — Phase 9 |
| `API-PRIVATE-002` | transport, executor, native body, replayability remain private | future auth/pagination internals and full recertification — Phases 6, 8-10 |
| `SEC-THREAT-001` | Phase 4 protected body values excluded | credential/provider/limiter/pagination values — Phases 6-8 |
| `SEC-BODY-002` | chunks, multipart values/filenames, producer text excluded from current surfaces | limiter contexts and pagination errors — Phases 7-8 |
| `SEC-RESP-003` | chunks excluded from errors/sources/hooks/debug/completion/wrappers | limiter contexts — Phase 7 |
| `SEC-HOOK-001` | current hooks/debug are sanitized metadata-only | provider/limiter/pagination integration — Phases 6-8 |
| `SEC-FAIL-001` | body/response/media/arithmetic failures close safely | public-header, auth, limiter, pagination cases — Phases 5-8 |
| `SEC-EXT-001` | current factory/media obligations documented | complete extension certification — Phase 9 |
| `SEC-TEST-001` | Phase 4 cross-surface body sentinels | limiter/pagination matrices and release audit — Phases 7, 8, 10 |
| `BOUND-BODY-007` | application limits stay positive/finite | production provider body paths — Phase 6 |
| `BOUND-BODY-011` | gzip post-decompression stream accounting | Brotli/Deflate maintained runtime probes — Phase 10 |
| `EXEC-CREATE-001`, `EXEC-CREATE-002` | no URL/body/factory/stream/hook/native/HTTP eager work | production provider and limiter exclusions — Phases 6-7 |
| `EXEC-HEAD-001` | body/media/TLS preflight precedes effects | Phase 5 shaping and Phase 6 auth collision preflight |
| `EXEC-HOOK-001` | veto prevents every current body/native effect | provider and limiter cutoffs — Phases 6-7 |
| `EXEC-BODY-001` | all current bodies precede native activity | credential and quota ordering — Phases 6-7 |
| `EXEC-BODY-003` | once per current attempt, never hidden retry | actual recovery attempt reconstruction — Phase 6 |
| `EXEC-NATIVE-002` | hidden retry invisible to current lifecycle/factory | pagination invisibility — Phase 8 |
| `EXEC-RESPHEAD-001` | one head event for every current final response | recovery-attempt responses — Phase 6 |
| `EXEC-RESPHEAD-002` | current head observation before body polling | limiter observation — Phase 7 |
| `EXEC-PREC-001` | managed body precedence/current earliest failure | pagination natural-stop precedence — Phase 8 |
| `EXEC-DEADLINE-001` | all current result-determining work bounded | provider/limiter/recovery/page work — Phases 6-8 |
| `EXEC-COMPLETE-001` | direct completion once, fixed, outside deadline | page completion — Phase 8 |
| `EXEC-COMPLETE-005` | direct operation/status/elapsed metadata | page/aggregate metadata — Phase 8 |
| `EXEC-CANCEL-001` | hook/body/multipart/response cancellation | provider/limiter/page stages — Phases 6-8 |
| `EXEC-ORDER-001` | generated/internal current canonical path | shaping/auth/limiter/pagination/prepared-public rows — Phases 5-9 |
| `COMPAT-BUILD-001` | current core/macro/generated/extensions compile on 1.85 | later workflows/platform automation — Phases 5-10 |
| `CONF-COMPILE-001` | current generated categories compile | later generated categories — Phases 5-8 |
| `CONF-COMPILE-002` | current extension traits compile downstream | later traits/certification — Phase 9 |
| `CONF-COMPILE-003` | Phase 4 diagnostics have fixtures | later complete grammar diagnostics — Phases 5-8, 10 |
| `CONF-DOWNSTREAM-001` | buffered/stream/multipart generated workflows | shaping/auth/limiter/pagination workflows — Phases 5-8 |
| `CONF-DOWNSTREAM-002` | current codec/media/body/multipart extensions | later extensions/certification — Phases 6-9 |
| `CONF-REQWEST-001` | Phase 4 poll/error/cancel/multipart/retry probes | recovery and later assumptions — Phases 6-10 |
| `CONF-ORDER-001` | implemented Phase 4 matrix rows | provider/limiter/pagination/prepared rows — Phases 6-9 |
| `CONF-SEC-001` | Phase 4 retained sources recursively sanitized | provider/limiter/pagination/release chains — Phases 6-10 |
| `CONF-MSRV-001` | all current signatures/workflows on Rust 1.85 | later signatures and platform automation — Phases 5-10 |
| `CONF-DOC-001` | current docs/doctests/rendered Rustdoc | later canonical examples — Phases 5-10 |
| `CONF-TRACE-006` | every Phase 4 row has immutable artifact ownership | release-final evidence completeness — Phase 10 |

## Phase 5 readiness

Phase 4 leaves one typed request/executor/body/response architecture ready for the next approved
slice. Phase 5 remains next and unstarted. It owns remaining client/scope/endpoint values, query and
public-header shaping, host prefixes, optional/defaulted values, and related compiler surfaces.
Nothing in this phase starts that work.

`PHASE_4_COMPLETE_WITH_LIMITATIONS`

`PHASE_5_NOT_STARTED`
