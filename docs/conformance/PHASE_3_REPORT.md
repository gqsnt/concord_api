# Phase 3 Integrated Conformance Report

## Verdict

`PHASE_3_COMPLETE_WITH_LIMITATIONS`

All Phase 3 exit criteria are satisfied. The limitations below are explicit boundaries of broader
0.1 requirements whose remaining categories belong to Phases 4-10; none invalidates a Phase 3 exit
criterion. Phase 4 has not started.

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable evidence identity

- Starting Phase 3 HEAD and Phase 2 anchor:
  `16b72d57b6fa4c5df629b74ca8605ba71b4c716d`
- Phase 2 corrected source:
  `ea1ce944fe1d2622db7a66600d734ac0dc04784f`
- Phase 2 corrected evidence/report:
  `f339d0e09d86d71c9ed1ea3a46a6092b4dffdea4`
- Phase 3 tested source:
  `c9521f649c91df0a0bae9786cba101f4d526692b`
- Phase 3 validation/evidence time: `2026-07-17T19:17:20Z`
- Phase 3 evidence/report commit: `7b958cde82cf138cb91ca2d33499679233f42358`
- Phase 3 evidence anchor: this report's documentation-only follow-up
- Branch: `master`
- Initial working tree: clean
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Cargo resolver: 3
- Nextest exercised: 0.9.140; repository minimum 0.9.115
- Managed HTTP baseline: Reqwest 0.13.4 with default features disabled

The source tree was clean when integrated validation began and remained unchanged throughout the
final validation matrix. Every Phase 3 evidence row points to the tested source commit above rather
than a later documentation commit.

## Phase 3 commits

| Unit | Commit | Subject | Purpose |
|---|---|---|---|
| Phase 2 follow-up / Unit 3.0 | `9a647867d5629ea077b2fefebea606aef510befc` | `test: reconcile phase2 coverage and UI fixtures` | Corrected coverage paths and methods; LF-normalized edited guidance; migrated source-only core Cargo failures to Trybuild; retained genuine feature/dependency fixtures; narrowed Nextest overrides. |
| Buffered vertical slice / Units 3.1-3.3 | `c9521f649c91df0a0bae9786cba101f4d526692b` | `feat: deliver buffered codec vertical slice` | Atomically advanced the core/macro generated contract to version 3 and connected codec contracts, one buffered body model, one executor, bounded decoding, generated POST body syntax, tests, features, and Rustdoc proof. |

The core body ABI, production executor, macro model/codegen, generated contract version, and their
end-to-end tests were committed together because splitting them would create intermediate
core/macro contract mismatches.

## Contract map and authority review

The complete pre-source contract map is the ignored working artifact
`git_diff/phase3-contract-map.md`. It was finished before Rust changes and is intentionally not
committed.

It inventories 126 exact known requirements:

- body/media/encoding/decoding:
  `PROD-CORE-002`, `PROD-CORE-003`, `API-CODEC-001`, `API-CODEC-002`,
  `API-CODEC-003`, `API-BODY-001`, `API-RESP-001`, `PROD-MODEL-001`,
  `PROD-STATE-003`, `EXEC-CREATE-001`, `EXEC-CREATE-002`, `EXEC-BODYMETA-001`,
  `EXEC-HOOK-001`, `EXEC-HOOK-002`, `EXEC-BODY-001`, `EXEC-BODY-002`,
  `EXEC-BUFFER-003`, `GEN-TERM-001`, `GEN-TERM-002`;
- limits/status:
  `PROD-GUAR-004`, `PROD-GUAR-010`, `API-RUNTIME-001`, `BOUND-BODY-001`,
  `BOUND-BODY-002`, `BOUND-BODY-006`, `BOUND-BODY-007`, `BOUND-BODY-011`,
  `BOUND-CODEC-001`, `EXEC-BUFFER-001`, `EXEC-BUFFER-002`,
  `EXEC-NOCONTENT-001`, `EXEC-STATUS-001`, `EXEC-STATUS-002`, `EXEC-STATUS-003`,
  `EXEC-STATUS-004`, `EXEC-STATUS-005`, `SEC-RESP-007`, `SEC-RESP-008`;
- generated/DSL:
  `GEN-TYPE-001`, `GEN-TYPE-002`, `GEN-END-001`, `GEN-END-002`, `GEN-END-003`,
  `GEN-REQ-001`, `GEN-EXPL-001`, `GEN-EXPL-002`, `GEN-EXPL-003`,
  `GEN-NAME-002`, `GEN-OWN-001`, `GEN-DOC-001`, `GEN-DOC-002`,
  `DSL-END-001`, `DSL-END-002`, `DSL-END-003`, `DSL-PARAM-001`,
  `DSL-PARAM-002`, `DSL-BODY-001`;
- feature/compatibility:
  `FEAT-JSON-001`, `FEAT-DIAG-001`, `FEAT-NODEFAULT-001`, `FEAT-DOC-001`,
  `COMPAT-CRATE-001`, `COMPAT-RUST-001`, `COMPAT-RUST-002`,
  `COMPAT-REQWEST-001`, `COMPAT-MACRO-001`, `COMPAT-MACRO-002`,
  `COMPAT-MACRO-003`, `COMPAT-GEN-001`, `COMPAT-GEN-002`,
  `COMPAT-SURF-001`, `COMPAT-BUILD-001`, `COMPAT-DEP-001`,
  `COMPAT-DEP-002`, `COMPAT-DEPS-001`;
- safety/lifecycle:
  `PROD-GUAR-002`, `PROD-GUAR-011`, `SEC-THREAT-001`, `SEC-THREAT-002`,
  `SEC-BODY-001`, `SEC-BODY-002`, `SEC-RESP-001`, `SEC-RESP-002`,
  `SEC-RESP-003`, `SEC-RESP-004`, `SEC-HOOK-001`, `SEC-TEST-001`,
  `EXEC-ORDER-001`, `EXEC-DEADLINE-001`, `EXEC-DEADLINE-004`,
  `EXEC-RESPHEAD-001`, `EXEC-RESPHEAD-002`, `EXEC-PREC-001`,
  `EXEC-COMPLETE-001`, `EXEC-COMPLETE-005`, `EXEC-CANCEL-001`;
- extension/conformance:
  `API-NS-001`, `API-REQ-001`, `API-REQ-002`, `API-PRIVATE-001`,
  `API-PRIVATE-002`, `CONF-COMPILE-001`, `CONF-COMPILE-002`,
  `CONF-COMPILE-003`, `CONF-DOWNSTREAM-001`, `CONF-DOWNSTREAM-002`,
  `CONF-DET-001`, `DOC-ID-003`, `DOC-CHG-003`, `CONF-CORE-002`,
  `CONF-TRACE-001`, `CONF-TRACE-005`, `CONF-TRACE-006`, `CONF-PARSER-001`,
  `CONF-SEMA-001`, `CONF-REQWEST-001`, `CONF-ORDER-001`, `CONF-SEC-001`,
  `CONF-MSRV-001`, `CONF-DOC-001`, `CONF-TEST-001`, `CONF-CI-001`,
  `CONF-CI-002`, `CONF-CI-003`, `CONF-CI-004`, `CONF-VERTICAL-001`.

No strategic contradiction required a decision. The authority review reconciled automatic
successful decoding with explicit complete terminal-body decoding through the one public
`DecodeBody` trait; static media metadata before veto with expensive encoding after veto; request
limits after codec completion but before native construction; one public header map for media
ownership; core-owned JSON feature activation; normative POST body examples; and metadata
terminals for the only currently implemented buffered/bytes/no-content response modes.

## Phase 2 follow-up

Coverage rows `GEN-END-003`, `GEN-EXPL-001`, `GEN-EXPL-002`, `GEN-DOC-001`,
`GEN-DOC-002`, and `GEN-NAME-002` now name actual macro UI, generated runtime, semantic, and
rendered Rustdoc paths rather than nonexistent example or Rustdoc files. Their Phase 3 notes now
name exact current evidence. The obsolete M0-M8 taxonomy was not globally rewritten: reviewed rows
use explicit P2/P3/P4/P5 ownership while untouched historical/future rows retain their planning
labels.

`REQUIREMENT_COVERAGE_PLAN_0.1.csv` was substantively reviewed and normalized to LF in Unit 3.0.
The contributor guide was also substantively updated and LF-normalized. Historical evidence rows
were not rewritten.

The macro Cargo fixtures remain `renamed_core`, `missing_core`, and `contract_mismatch`;
`json_disabled` is additionally justified because the caller's Cargo feature graph is the test.
Core `config_features` and `development_features` remain Cargo fixtures for feature topology.
Source-only `raw_escape` and `status_body_traits` cases moved into the ordinary core Trybuild UI
suite. Nextest extended timeouts apply only to genuine Cargo-spawning binaries; core UI tests use
ordinary execution. No nested `cargo test` process was added.

## Public codec surface

`concord_core::advanced` exports the exact public `MediaType`, `EncodeContext`,
`EncodeBody`, `DecodeContext`, `DecodeBody`, and `CodecError` contracts plus common wire
types. `Text`, `OctetStream`, and feature-gated `Json<T>` are normal prelude and advanced
exports. `CodecError` retains private text and structural safe `Display`, `Debug`, and
`source()` behavior.

`Text` accepts `String`, emits UTF-8 with `text/plain; charset=utf-8`, defaults
`Accept: text/plain`, decodes exact UTF-8, and intentionally does not reject another or missing
successful response media type. Invalid UTF-8 becomes a safe response-decoding error.

Under `json`, `Json<T>` serializes/deserializes through optional Serde dependencies, owns
`application/json`, and accepts valid `application/json` or `application/*+json` media types
with arbitrary valid parameters. JSON-disabled builds expose no `Json<T>` and activate none of
`mime`, `serde`, or `serde_json`.

## Request body model and limits

The existing generated-private `BodySpec`, `BodyMetadata`, and skeletal `PreparedBody` were
evolved in place. One `PreparedBody` stores empty or finite `Bytes`; one generic
`CodecBody<C>` handles Text, JSON, and downstream codecs. There is no codec-specific production
body architecture.

Body metadata contains only kind, optional known length, and static content type. It is prepared
without invoking the codec. After hook approval, the encoder runs once with endpoint, method, and
the positive configured limit. Encoder failure or first byte above the limit prevents native
request construction and execution. Exact-limit and below-limit bodies succeed.

The one `PreparedHead` header map inserts codec `Accept` only when absent. A codec
`Content-Type` owns that header and conflicts fail during public preparation. Invalid static
codec header values fail safely before hooks. This representation is ready for later public-header
shaping without a codec-only parallel header system.

## Production execution lifecycle

The only production function remains `execute_request`. The Phase 3 path is:

```text
first poll: OperationId + start/deadline
→ deferred dynamic route and safe public-head preparation
→ body metadata and media ownership validation
→ before_send under the remaining budget
→ one buffered encode/materialization
→ request-body limit validation
→ one native request construction
→ one managed ApplicationHttp / Reqwest execution
→ safe response-head debug and hook
→ status classification
→ non-success complete-only terminal capture
  or NoContent drop without polling
  or known-length preflight + one bounded successful collector
→ content contract validation before polling
→ DecodeBody only after complete accepted collection
→ fixed typed result/status/elapsed
→ on_complete outside the result-determining deadline
→ direct value or DecodedResponse<T>
```

Request construction stays side-effect-free; route formatting, identity, and timing begin on first
poll. Veto prevents encoder and native calls. Synchronous encoder, media validator, and decoder
work is checked against the deadline immediately after return. Completion cannot replace the fixed
result. Cancellation and panic retain the approved no-delivery allowance.

## Response handling

One `collect_bounded` loop serves `Bytes` and decoded buffered successes. Known excessive
length fails before polling. For unknown length, the first excess chunk is rejected before it
enters the buffer visible to a decoder or caller. Media validation follows known-length preflight
and precedes body polling. Decode occurs only after complete accepted collection.

`Bytes` remains bounded for direct and metadata results. `NoContent` accepts every 2xx and
drops the native response without polling its body. Non-2xx responses remain a separate finite
complete-only path; their known status stays primary when capture fails, and explicit
`StatusBody::decode_body::<C>()` continues to use the same public `DecodeBody` contract.

`DecodedResponse<T>` remains the stable metadata wrapper and does not format application `T`.
All implemented Phase 3 modes add metadata beyond their direct output and therefore retain the
stable `ResponseRequest<E>` terminal. Streaming is not present; Phase 4 must return its own
metadata-bearing direct stream result and must not create a second buffered terminal.

## Generated DSL and API

The existing parser/model/codegen pipeline now accepts:

```text
GET or POST Endpoint(required..., body: Text) -> Text
POST Endpoint(required..., body: Json<T>) -> Json<U>
POST Endpoint(required..., body: crate::Codec) -> crate::Decoder
```

The body must be final. `NoContent` cannot be a request body. `Bytes` and `NoContent`
response modes remain. Stream and multipart markers are rejected as deferred Phase 4 work.
Unrelated methods, values, headers, query shaping, host labels, auth, pagination, and prepared
endpoints remain outside the accepted subset.

Generated Text input is `String`; JSON input is its inner `T`; custom input is
`<C as EncodeBody>::Input`. Facade and explicit constructors put body last and return the same
`concord_core::Request<E>`. Direct output and `DecodedResponse<T>` follow the declared
response codec.

The API-specific sealed endpoint capability remains compile-time enforced: cross-client endpoints,
arbitrary values, and downstream forgery fail; same-client explicit endpoints work. No runtime
brand, `Any`, public transport, or public executor was added. Normal and renamed runtime
dependencies, nested and repeated macro invocations, and contract mismatch detection all pass.
Generated contract version is 3.

Rendered renamed-dependency Rustdoc is scanned for and excludes `concord_core::__private`, the
private seal, `ClientCore`, `EndpointSpec`, `ResponseHandler`, `CodecBody`,
`BufferedResponse`, `PreparedBody`, Reqwest types, and internal executor/body names.

## Downstream extension proof

`concord_core/tests/codec_extensions.rs` acts as an external crate and implements a custom media
marker plus request and response codecs using only `concord_core::advanced` and ordinary public
wire types. It has no `__private` import and passes without default features or JSON.

`concord_macros/tests/generated_surface.rs` then executes a custom codec through an actual
generated POST endpoint, both facade and explicit, over test-owned loopback HTTP. The renamed-core
Cargo fixture also compiles Text and custom codecs through the renamed runtime path.

## Deterministic and real HTTP evidence

Deterministic production-boundary tests prove exact encoder/validator/decoder counts, hook order,
request count, veto, encoding and request-limit cutoffs, response-limit cutoffs, media mismatch,
safe decoder failure, timeout phase, completion count, safe metadata, and request-local isolation.

Real local Reqwest tests prove transmitted bytes, POST, `Content-Type`, `Accept`, Text, JSON,
custom codecs, exact/known/unknown response limits, first-excess rejection, media mismatch before
body polling, Bytes, NoContent, and terminal status behavior. Servers use ephemeral test-owned
loopback sockets; no external network or fixed global port is used.

Sentinels cover request/success/terminal bytes and codec-authored messages across `Display`,
`Debug`, recursive `source()`, hooks, debug observations, mismatch messages, and generated
Rustdoc. Concord-generated messages copy none of them. Codec-authored logging or side effects remain
the explicitly documented trusted downstream extension boundary.

## Integrated validation

The following final commands passed against exact source
`c9521f649c91df0a0bae9786cba101f4d526692b`:

| Command | Result |
|---|---|
| `just fmt-check` | passed |
| `just check` | passed; workspace all targets default |
| `just check-all` | passed; workspace all targets all features |
| `just check-no-default` | passed; core all targets no defaults |
| `just clippy` | passed with warnings denied |
| `just clippy-all` | passed with warnings denied |
| `just test` / release CI-default stage | passed; Nextest 286/286 across 31 binaries |
| `just test-all` | passed; CI-profile Nextest 289/289 across 31 binaries |
| `just test-no-default` | passed 1/1; 182 filtered |
| `just doctest` | passed; core 2/2; other libraries zero |
| `just doctest-all` | passed; core 2/2; other libraries zero |
| `just doc` | passed for core macros and test support with warnings denied |
| `just doc-all` | passed for all three libraries with warnings denied |
| `just deny` | passed advisories bans licenses and sources; allowed duplicate-version warnings only |
| `just diff-check` | passed |
| `just release` | passed the composed matrix above |
| `cargo nextest run -p concord_core --lib` | passed 189/189 |
| `cargo nextest run -p concord_core --lib --all-features` | passed 195/195 |
| `cargo nextest run -p concord_core --test codec_extensions` | passed 2/2 |
| `cargo nextest run -p concord_core --test status_body_contract` | passed 12/12 |
| `cargo nextest run -p concord_core --test ui` | passed 1/1 Trybuild harness |
| `cargo nextest run -p concord_core --test config_methods` | passed 1/1 Cargo feature harness |
| `cargo check -p concord_core --all-targets --no-default-features --features json` | passed |
| JSON-only no-default exact Nextest selection | passed 3/3; 183 filtered |
| `cargo nextest run -p concord_core --no-default-features --test codec_extensions` | passed 2/2 |
| JSON-disabled/enabled `cargo tree` depth-one commands | passed; optional dependencies absent/present exactly |
| `cargo nextest run -p concord_macros --lib` | passed 9/9 parser/model/codegen tests |
| `cargo nextest run -p concord_macros --test generated_surface` | passed 8/8 |
| `cargo nextest run -p concord_macros --test ui` | passed 6/6 |
| `cargo nextest run -p concord_macros --test compile` | passed 5/5 Cargo/Rustdoc fixtures |
| `cargo nextest run -p concord_macros --test dependency_policy` | passed 3/3 |
| `cargo metadata --locked --no-deps --format-version 1` | passed manifest/version audit |
| `cargo tree -p concord_macros --edges normal` | passed macro/runtime separation audit |
| `cargo tree --workspace --edges normal,build` | passed production/build transport ownership audit |

The exact JSON-only filter selected:

```text
codec::tests::json_accepts_standard_and_structured_suffix_media_types
codec::tests::json_round_trips_without_exposing_serde_failures
execute::tests::real_reqwest_json_round_trip_is_feature_owned
```

Before the source commit, a focused six-test no-default execution selection failed because the
Reqwest development dependency topology left no process-global Rustls crypto provider installed.
This was a test-environment defect rather than a product failure. The shared real/deterministic
test helpers now install the ring provider only when `default-tls` is absent. The same six tests
then passed 6/6; the final no-default check, JSON-only 3/3, custom-codec 2/2, and complete release
matrix all passed. No validation is unavailable.

`cargo deny` emitted only the pre-existing accepted non-failing duplicate-version warnings for
`core-foundation` and `windows-sys`.

## Source self-review and architecture negatives

The complete staged patch (2,216 insertions and 190 deletions across 47 files) and committed binary
patch were generated and inspected. Searches and public Rustdoc review confirm:

- exactly one `Request<E>` pending model and one `execute_request` production executor;
- exactly one `BodyMetadata` / `PreparedBody` / generic `CodecBody<C>` body architecture;
- exactly one public decoder trait: `DecodeBody`;
- exactly one successful buffered collection loop shared with Bytes;
- one prepared public head and header map;
- no public transport or executor;
- no exposed Reqwest client request or response;
- no runtime `Any`;
- no streaming body/response implementation;
- no body factory or multipart implementation;
- no authentication or recovery implementation;
- no rate-limiter implementation;
- no pagination execution;
- no prepared public endpoint API.

Existing reduced auth/pagination models and final hook/configuration vocabulary remain unchanged
proof foundations from earlier phases; they are not production implementations.

Self-review corrected three issues before the source commit: no-default test TLS provider setup,
generated Rustdoc sentinels for the new private body/response names, and parser diagnostics that
distinguish Phase 3 methods from deferred streaming/multipart markers. Clippy, UI, feature, and full
release reruns passed after correction.

## Evidence and coverage

The evidence catalog preserves its first 319 historical rows in identical order and text, including
all eight `P2-AUDIT-*` rows pointing to
`ea1ce944fe1d2622db7a66600d734ac0dc04784f`, `EXEC-CANCEL-001` partial evidence, and
`GEN-NAME-001` partial evidence.

Phase 3 appends 126 unique `P3-*` rows: 102 direct and 24 explicitly partial. The resulting catalog
contains 445 rows and 445 unique evidence IDs, with zero unknown normative requirement IDs. Every
new row points to `c9521f649c91df0a0bae9786cba101f4d526692b`.

The coverage plan now names current parser, codegen, UI, runtime, feature, and Rustdoc paths for the
buffered boundary and distinguishes current direct proof from future Phase 4/5/release ownership.
It remains LF-only. Historical evidence paths were not rewritten.

## Remaining limitations and Phase 4 readiness

The 24 partial Phase 3 rows are:

`PROD-CORE-003`, `PROD-GUAR-004`, `PROD-GUAR-010`, `API-BODY-001`,
`BOUND-BODY-006`, `BOUND-BODY-007`, `GEN-EXPL-002`, `GEN-NAME-002`,
`GEN-DOC-002`, `EXEC-ORDER-001`, `EXEC-DEADLINE-001`, `EXEC-DEADLINE-004`,
`EXEC-CANCEL-001`, `SEC-TEST-001`, `CONF-COMPILE-001`, `CONF-COMPILE-002`,
`CONF-COMPILE-003`, `CONF-DOWNSTREAM-001`, `CONF-DOWNSTREAM-002`,
`CONF-REQWEST-001`, `CONF-ORDER-001`, `CONF-SEC-001`, `CONF-MSRV-001`, and
`CONF-DOC-001`.

Their remaining portions belong to Phase 4 streaming/factories/multipart/replayability; Phase 5
remaining values and request shaping; Phases 6-9 auth, rate limiting, pagination, and prepared
endpoints; or Phase 10 release-wide platform/documentation evidence.

Phase 4 can extend the existing `BodySpec`, `PreparedBody`, response-handler capability, head,
and single executor. It must not replace the public codec contracts or add a second request,
decoder, body, collector, or executor architecture.

`PHASE_4_NOT_STARTED`
