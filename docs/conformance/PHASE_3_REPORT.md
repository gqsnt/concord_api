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
- Original Phase 3 evidence anchor: `304709bc9cde4d806475f8a18080969efd20994e`
- Corrected Phase 3 source: `7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f`
- Corrected validation/evidence time: `2026-07-17T21:41:27Z`
- Corrected Phase 3 evidence/report commit: `08fd7adeacab1efcf2b1543a04574b796ab4010e`
- Corrected Phase 3 evidence anchor: this report's documentation-only correction follow-up
- Branch: `master`
- Initial working tree: clean
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Cargo resolver: 3
- Nextest exercised: 0.9.140; repository minimum 0.9.115
- Managed HTTP baseline: Reqwest 0.13.4 with default features disabled

The source tree was clean when corrected validation began and remained unchanged throughout the
final validation matrix. Corrected rows point to the immutable corrected source commit rather than
a later documentation commit; accurate original Phase 3 rows retain their original immutable
source reference. Git history preserves the original source, evidence, and anchor commits.

## Phase 3 commits

| Unit | Commit | Subject | Purpose |
|---|---|---|---|
| Phase 2 follow-up / Unit 3.0 | `9a647867d5629ea077b2fefebea606aef510befc` | `test: reconcile phase2 coverage and UI fixtures` | Corrected coverage paths and methods; LF-normalized edited guidance; migrated source-only core Cargo failures to Trybuild; retained genuine feature/dependency fixtures; narrowed Nextest overrides. |
| Buffered vertical slice / Units 3.1-3.3 | `c9521f649c91df0a0bae9786cba101f4d526692b` | `feat: deliver buffered codec vertical slice` | Atomically advanced the core/macro generated contract to version 3 and connected codec contracts, one buffered body model, one executor, bounded decoding, generated POST body syntax, tests, features, and Rustdoc proof. |
| Controller correction | `7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f` | `fix: enforce buffered response terminal capability` | Added the sealed metadata-terminal capability, positive and negative compiler/runtime proof, completed development-namespace UI migration, and extended the generated Rustdoc privacy sentinel. |

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

The independent controller review found three blocking documentation/source defects in the
original Phase 3 state: `.response()` was an unconditional inherent method, the retained
`development_features` Cargo project still executed one source-only namespace failure, and several
direct evidence labels covered only proper subsets of their normative requirements. The corrected
design uses the existing sealed response-handler boundary, moves the namespace failure into core
Trybuild, and reclassifies every identified subset without implementing deferred public headers,
grammar, extensions, or release-final behavior. The broader 126-row audit additionally found
compound lifecycle, protected-value, MSRV, and extension requirements whose remaining body-mode,
provider, limiter, pagination, certification, or release-wide clauses belong to Phases 4 and 6-10.

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
suite during Unit 3.0. The correction also moved `development_namespace_escape` from
`development_features/src/bin` into that UI suite and removed its dedicated nested Cargo check.
The retained development fixture now proves only feature presence, feature absence, and generated
client compilation under the feature graph. Nextest extended timeouts apply only to genuine
Cargo-spawning binaries; core UI tests use ordinary execution. No nested `cargo test` process was
added.

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
`Content-Type` owns that header in the current internal preparation model, and invalid static
codec header values fail safely before hooks. This proves shared ownership architecture, not the
complete `API-CODEC-003` public precedence contract: application-provided public `Accept` and
`Content-Type` inputs remain deferred to Phase 5. No codec-only parallel header system was added.

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
The generated-private sealed `MetadataResponse` capability is implemented only by
`BufferedResponse<C>`, `BytesResponse`, and `NoContentResponse`. The inherent `.response()` method
exists only when `E::Response` has that capability, while the independent `IntoFuture` path for
direct await is unchanged. A focused core UI case binds only `EndpointSpec` and fails specifically
because `<E as EndpointSpec>::Response: MetadataResponse` is unsatisfied; generated Text, JSON,
custom, Bytes, and NoContent runtime tests prove the supported side. Streaming is not present;
Phase 4's direct stream result will already carry metadata and will not acquire or remove this
buffered terminal.

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
private seal, `ClientCore`, `EndpointSpec`, `MetadataResponse`, `ResponseHandler`, `CodecBody`,
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

The retained gzip runtime probe proves that limits count bytes yielded after Reqwest
decompression. Brotli and Deflate remain independently feature-wired but lack equivalent
maintained runtime probes, so `BOUND-BODY-011` is partial; Phase 10 owns completion of that
algorithm matrix.

Sentinels cover request/success/terminal bytes and codec-authored messages across `Display`,
`Debug`, recursive `source()`, hooks, debug observations, mismatch messages, and generated
Rustdoc. Concord-generated messages copy none of them. Codec-authored logging or side effects remain
the explicitly documented trusted downstream extension boundary.

## Integrated validation

The following corrected commands passed against exact source
`7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f`:

| Command | Result |
|---|---|
| `just fmt-check` | passed |
| `just check` | passed; workspace all targets default |
| `just check-all` | passed; workspace all targets all features |
| `just check-no-default` | passed; core all targets no defaults |
| `just clippy` | passed with warnings denied |
| `just clippy-all` | passed with warnings denied |
| `just test` | passed; Nextest 285/285 across 31 binaries |
| `just test-all` | passed; CI-profile Nextest 288/288 across 31 binaries |
| `just test-no-default` | passed 1/1; 182 filtered |
| `just doctest` | passed; core 2/2; other libraries zero |
| `just doctest-all` | passed; core 2/2; other libraries zero |
| `just doc` | passed for core macros and test support with warnings denied |
| `just doc-all` | passed for all three libraries with warnings denied |
| `just deny` | passed advisories bans licenses and sources; allowed duplicate-version warnings only |
| `just diff-check` | passed |
| `just release` | passed; CI-default 285/285, all-features 288/288, no-default 1/1, doctests 2/2 |
| `cargo nextest run -p concord_core --test ui` | passed 1/1 harness; seven default-feature compile-fail cases including response capability and development namespace |
| `cargo nextest run -p concord_core --test development` | passed 2/2 genuine Cargo feature-topology cases |
| `cargo nextest run -p concord_core --test config_methods` | passed 1/1 genuine Cargo feature-topology harness |
| `cargo nextest run -p concord_core --all-features --test codec_extensions` | passed 2/2 |
| `cargo nextest run -p concord_core --no-default-features --test codec_extensions` | passed 2/2; custom codecs require no JSON feature |
| `cargo check -p concord_core --all-targets --no-default-features --features json` | passed; JSON-only no-default surface compiled |
| `cargo nextest run -p concord_core --no-default-features --features json -E 'test(codec::tests::json_accepts_standard_and_structured_suffix_media_types) \| test(codec::tests::json_round_trips_without_exposing_serde_failures) \| test(execute::tests::real_reqwest_json_round_trip_is_feature_owned)'` | passed 3/3; 242 skipped |
| `cargo nextest run -p concord_core --all-features --test status_body_contract` | passed 12/12 |
| `cargo nextest run -p concord_core --all-features --test error_redaction --test hook_redaction --test deterministic_redaction` | passed 3/3 |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(~execute::tests::)'` | passed 23/23; 172 skipped |
| `cargo nextest run -p concord_macros --test generated_surface` | passed 8/8 |
| `cargo nextest run -p concord_macros --test ui` | passed 6/6 |
| `cargo nextest run -p concord_macros --test compile` | passed 5/5 Cargo/Rustdoc fixtures |
| `cargo nextest show-config version` | passed; current 0.9.140 satisfies required 0.9.115 |
| `cargo nextest show-config test-groups` | passed; only five Cargo-spawning binaries receive the bounded override; core UI does not |

Default, all-feature, and no-default checks cover JSON enabled and disabled. The generated runtime
suite covers direct and metadata Text, JSON, custom, Bytes, and NoContent behavior; the compile
fixture suite covers JSON-disabled diagnostics, renamed runtime dependency, missing dependency,
generated contract mismatch, and generated Rustdoc privacy. No corrected validation is unavailable.

`cargo deny` emitted only the pre-existing accepted non-failing duplicate-version warnings for
`core-foundation` and `windows-sys`.

## Source self-review and architecture negatives

The original complete Phase 3 patch and the focused correction patch (78 insertions and 32
deletions across 14 files) were generated and inspected. Searches and public Rustdoc review
confirm:

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

Correction self-review verified that direct await is unchanged and no runtime capability check,
second response model, or Phase 4 marker was introduced. One pre-commit all-feature UI run exposed
a feature-dependent compiler suggestion for the development namespace case. The source-only
namespace proof was scoped to the ordinary feature configuration, while the retained feature
fixture independently proves enabled/disabled topology; the exact all-feature command then passed
254/254. The Rustdoc sentinel was extended for `MetadataResponse`. Clippy, UI, feature, focused
runtime, and full release reruns passed after correction.

## Evidence and coverage

The evidence catalog preserves its first 319 historical rows in identical order and text, including
all eight `P2-AUDIT-*` rows pointing to
`ea1ce944fe1d2622db7a66600d734ac0dc04784f`, `EXEC-CANCEL-001` partial evidence, and
`GEN-NAME-001` partial evidence.

Phase 3 contains 126 unique `P3-*` rows covering 126 exact requirements: 78 direct and 48
explicitly partial after correction. The resulting catalog contains 445 rows and 445 unique
evidence IDs, with zero unknown normative requirement IDs. Accurate original rows retain source
`c9521f649c91df0a0bae9786cba101f4d526692b`; the 28 materially corrected rows reference
`7ac4a1c6e18a080f4810b93ae4bc6aa02f52ed8f`. The correction commit supersedes only those original
classifications/descriptions and leaves the original evidence commit intact in Git history.

| Evidence row | Original label | Corrected label | Correction |
|---|---|---|---|
| `P3-DEC-005` / `GEN-TERM-002` | direct | direct | Re-anchored to the sealed method-availability bound, unsupported UI diagnostic, and supported generated runtime cases. |
| `P3-MEDIA-001` / `API-CODEC-003` | direct | partial | Phase 3 proves internal shared ownership; Phase 5 must prove public user-header precedence and conflicts. |
| `P3-RESP-002` / `BOUND-BODY-011` | direct | partial | Gzip has runtime proof; Brotli and Deflate have feature wiring but await Phase 10 runtime probes. |
| `P3-FEAT-014` / `COMPAT-SURF-001` | direct | partial | Custom codecs are certified; every documented extension workflow remains a Phase 9 obligation. |
| `P3-EXT-003` / `API-REQ-002` | direct | direct | Re-anchored to the hidden capability bound plus core UI and generated Rustdoc privacy proof. |
| `P3-EXT-005` / `API-PRIVATE-002` | direct | direct | Re-anchored to the completed source-only namespace migration and response-capability privacy proof. |
| `P3-CONF-006` / `CONF-TRACE-006` | direct | partial | Current rows are exact; release-final missing-evidence rejection remains Phase 10. |
| `P3-CONF-007` / `CONF-PARSER-001` | direct | partial | Only the implemented buffered grammar is covered; exhaustive grammar coverage remains Phase 10. |
| `P3-CONF-008` / `CONF-SEMA-001` | direct | partial | Buffered normalization is covered; complete inherited configuration resolution remains Phase 5 with Phase 10 closure. |
| `P3-ENC-001` / `EXEC-CREATE-001` | direct | partial | Current creation excludes hooks, codecs, and network activity; provider and limiter exclusions remain Phases 6 and 7. |
| `P3-ENC-002` / `EXEC-CREATE-002` | direct | partial | Current first-poll proof covers URL, codec, hook, native request, and HTTP work; factories, providers, and limiters remain Phases 4, 6, and 7. |
| `P3-ENC-003` / `EXEC-BODYMETA-001` | direct | partial | Buffered metadata preparation is non-materializing; factory, multipart, and stream forms remain Phase 4. |
| `P3-ENC-004` / `EXEC-HOOK-001` | direct | partial | Veto blocks buffered encoding and native execution; provider and limiter cutoff proof remains Phases 6 and 7. |
| `P3-ENC-006` / `EXEC-BODY-001` | direct | partial | Buffered materialization precedes native construction; credential-provider and limiter ordering remains Phases 6 and 7. |
| `P3-RESP-008` / `EXEC-STATUS-003` | direct | partial | Current non-success handling is status-primary; limiter observation of server categories remains Phase 7. |
| `P3-GEN-004` / `GEN-END-002` | direct | partial | Generated construction invokes no codec, hook, or HTTP work; provider and limiter non-invocation remains Phases 6 and 7. |
| `P3-FEAT-015` / `COMPAT-BUILD-001` | direct | partial | The current matrix passes on Rust 1.85; complete extension and release-platform MSRV coverage remains Phases 9 and 10. |
| `P3-SEC-001` / `PROD-GUAR-002` | direct | partial | Current body-bearing diagnostic surfaces are covered; streaming, limiter, pagination, and release-wide surfaces remain Phases 4, 7, 8, and 10. |
| `P3-SEC-005` / `SEC-BODY-001` | direct | partial | Buffered request bytes are excluded from diagnostics; multipart values remain Phase 4. |
| `P3-SEC-009` / `SEC-RESP-003` | direct | partial | Buffered and terminal response bytes are excluded from current surfaces; stream, raw, and limiter surfaces remain Phases 4, 7, and 10. |
| `P3-LIFE-004` / `EXEC-RESPHEAD-001` | direct | partial | Current attempts emit one response-head observation; authentication recovery attempts remain Phase 6. |
| `P3-LIFE-005` / `EXEC-RESPHEAD-002` | direct | partial | Observation precedes current status/body work; limiter observation ordering remains Phase 7. |
| `P3-LIFE-006` / `EXEC-PREC-001` | direct | partial | Current request failures retain the earliest phase; pagination natural-stop precedence remains Phase 8. |
| `P3-LIFE-007` / `EXEC-COMPLETE-001` | direct | partial | Current request operations emit one fixed-result completion; page-operation completion remains Phase 8. |
| `P3-LIFE-008` / `EXEC-COMPLETE-005` | direct | partial | Current request correlation metadata is stable through fixation; page and aggregate correlation remains Phase 8. |
| `P3-REQ-LIMIT-003` / `API-RUNTIME-001` | direct | direct | Re-anchored to the public configuration proof for every positive finite request, response, stream, provider, and pagination ceiling. |
| `P3-SEC-003` / `SEC-THREAT-001` | direct | partial | Current protected-value categories are covered; credential, provider, stream, multipart, limiter, pagination, extension, and release-wide surfaces remain Phases 4 and 6-10. |
| `P3-SEC-006` / `SEC-BODY-002` | direct | partial | Buffered codec input, output, and errors are covered; stream, multipart, producer, limiter, and pagination surfaces remain Phases 4, 7, 8, and 10. |

The coverage plan now names current parser, codegen, UI, runtime, feature, and Rustdoc paths for the
buffered boundary and distinguishes current direct proof from future Phase 4/5/release ownership.
It remains LF-only. Historical evidence paths were not rewritten.

## Remaining limitations and Phase 4 readiness

The 48 partial Phase 3 rows are:

`PROD-CORE-003`, `PROD-GUAR-002`, `PROD-GUAR-004`, `PROD-GUAR-010`,
`API-BODY-001`, `API-CODEC-003`, `BOUND-BODY-006`, `BOUND-BODY-007`,
`BOUND-BODY-011`, `GEN-END-002`, `GEN-EXPL-002`, `GEN-NAME-002`, `GEN-DOC-002`,
`EXEC-CREATE-001`, `EXEC-CREATE-002`, `EXEC-BODYMETA-001`, `EXEC-HOOK-001`,
`EXEC-BODY-001`, `EXEC-RESPHEAD-001`, `EXEC-RESPHEAD-002`, `EXEC-STATUS-003`,
`EXEC-PREC-001`, `EXEC-COMPLETE-001`, `EXEC-COMPLETE-005`, `EXEC-ORDER-001`,
`EXEC-DEADLINE-001`, `EXEC-DEADLINE-004`, `EXEC-CANCEL-001`, `SEC-THREAT-001`,
`SEC-BODY-001`, `SEC-BODY-002`, `SEC-RESP-003`, `SEC-TEST-001`,
`COMPAT-SURF-001`, `COMPAT-BUILD-001`,
`CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`,
`CONF-DOWNSTREAM-001`, `CONF-DOWNSTREAM-002`, `CONF-TRACE-006`,
`CONF-PARSER-001`, `CONF-SEMA-001`, `CONF-REQWEST-001`, `CONF-ORDER-001`,
`CONF-SEC-001`, `CONF-MSRV-001`, and `CONF-DOC-001`.

Their remaining portions belong to Phase 4 streaming/factories/multipart/replayability; Phase 5
public headers, inherited values, and request shaping; Phases 6-8 auth, rate limiting, and
pagination; Phase 9 prepared endpoints and complete extension certification; or Phase 10
compression-algorithm, grammar, platform, documentation, and release-final evidence.

Phase 4 can extend the existing `BodySpec`, `PreparedBody`, response-handler capability, head,
and single executor. It must not replace the public codec contracts or add a second request,
decoder, body, collector, or executor architecture.

`PHASE_4_NOT_STARTED`
