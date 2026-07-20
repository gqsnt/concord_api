# Phase 12 Conformance Report

## Verdict

`PHASE_12_COMPLETE_WITH_LIMITATIONS`

Phase 12 — Generic Body/Response Specifications and Integrated Streaming Records is complete.
The limitations are only the explicitly unimplemented Phase 13 lexical rate limits and Phase 14
product-completion and release-final work. The normative package remains
`Normative Implementation Candidate`; only Phase 14 may restore `Normative Release Final`.

Phase 13 — Lexically Scoped Rate Limits is active with status `NOT_STARTED`. Phase 12 implements no
Phase 13 or Phase 14 product behavior and starts no Phase 15 work.

## Identities and Git boundary

- Starting HEAD: `b66d2914609aec7b767481e09dedd5544b3d9130`
- Starting branch/tree: clean `master`
- Phase 12 authority commit: `95a1b9075061184ff2fa5807bb28b16a4d172277`
- Initial Phase 12 source/test commit: `e71ed0671359f443f5aa165391ff56bd10debdf2`
- Focused source-correction commit: `d1aadcebcad0a65d92c4ed4a42d6b0a9c1f034b2`
- Final immutable tested source commit: `d1aadcebcad0a65d92c4ed4a42d6b0a9c1f034b2`
- Phase 12 evidence/report/state commit: `EVIDENCE_COMMIT_PENDING`
- Phase 12 evidence anchor: `ANCHOR_COMMIT_PENDING`
- Compatible anchor-metadata descendant: the final two-file commit titled
  `docs: record phase 12 anchor`; its own identity is intentionally not self-embedded.
- Head policy: `anchor-must-be-an-ancestor`

The initial source identity passed all source, test, feature, dependency, MSRV, documentation, and
security work before the final source audit exposed three Unicode literals that Windows PowerShell
5.1 decoded incorrectly from a BOM-less UTF-8 audit script. The non-amended correction expresses
the same en/em dashes with ASCII-source Unicode code points. Direct self-test and Source-mode audits
then passed, and the complete matrix was rerun against the corrected immutable source.

No branch was created. No remote Git operation, push, pull, fetch, merge, rebase, amend, tag,
publication, deployment, credential access, or history rewrite occurred. One pre-freeze fixture
dependency check unintentionally printed `Updating crates.io index` before the command was corrected
to the maintained locked/offline path. It used no credentials and changed no tracked source; it is
not represented as authoritative validation. Every final fixture and dependency-topology proof ran
through the maintained locked/offline commands.

## Authority, lineage, and decisions

The authority owner is
`docs/normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md`. It records the prior title, prior
paragraph digest, disposition, replacement, normative owner, user-approved Phase 12 directive,
implementation phase, and historical-evidence treatment for every changed requirement.

Exactly 72 retained identifiers have explicit paragraph-version changes:

- `PROD-CORE-002`, `PROD-CORE-003`, `PROD-GUAR-002`, `PROD-GUAR-004`, `PROD-GUAR-008`,
  `PROD-GUAR-009`, `PROD-GUAR-010`
- `DSL-BODY-001`, `DSL-BODY-002`, `DSL-END-001`, `DSL-RESP-001`, `DSL-FEAT-001`,
  `DSL-FEAT-002`
- `API-CODEC-001`, `API-CODEC-002`, `API-CODEC-003`, `API-CODEC-004`, `API-BODY-001`,
  `API-BODY-002`, `API-BODY-003`, `API-RESP-001`, `API-PREP-001`, `API-PREP-003`
- `GEN-END-001`, `GEN-END-002`, `GEN-TERM-001`, `GEN-TERM-002`, `GEN-PAGE-003`,
  `GEN-PAGE-004`
- `SEC-BODY-001`, `SEC-BODY-002`, `SEC-RESP-003`, `SEC-RESP-006`, `SEC-RESP-007`,
  `SEC-RESP-008`, `BOUND-BODY-001`, `BOUND-BODY-002`, `BOUND-BODY-003`, `BOUND-BODY-006`,
  `BOUND-BODY-007`, `BOUND-BODY-011`
- `EXEC-BODYMETA-001`, `EXEC-BODY-001`, `EXEC-BODY-002`, `EXEC-BODY-003`, `EXEC-BODY-004`,
  `EXEC-BODY-005`, `EXEC-BUFFER-001`, `EXEC-BUFFER-002`, `EXEC-BUFFER-003`,
  `EXEC-NOCONTENT-001`, `EXEC-STREAM-001`, `EXEC-STREAM-002`, `EXEC-STREAM-003`,
  `EXEC-DEADLINE-001`, `EXEC-DEADLINE-005`, `EXEC-COMPLETE-003`, `EXEC-RAW-001`
- `FEAT-JSON-001`, `FEAT-MULTIPART-001`, `FEAT-COMPRESS-001`, `COMPAT-MACRO-001`,
  `COMPAT-SURF-001`, `COMPAT-BUILD-001`
- `CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`, `CONF-DOWNSTREAM-001`,
  `CONF-DOWNSTREAM-002`, `CONF-SEC-001`, `CONF-MSRV-001`, `CONF-DOC-001`

The exact supersessions are:

| Historical ID | Current replacement |
|---|---|
| `DSL-RESP-002` | `DSL-RESP-004` |
| `SEC-RESP-004` | `SEC-RESP-009` |
| `SEC-RESP-005` | `SEC-STREAM-001` |

Exactly 55 identifiers are new in Phase 12:

- `PROD-MODEL-008`, `PROD-GUAR-014`
- `DSL-BODY-003`, `DSL-RESP-003`, `DSL-RESP-004`, `DSL-FEAT-004`
- `API-BODYSPEC-001`, `API-BODYSPEC-002`, `API-BODYSPEC-003`, `API-BODYSPEC-004`
- `API-RESPSPEC-001`, `API-RESPSPEC-002`, `API-RESPSPEC-003`, `API-RESPSPEC-004`
- `API-RESP-002`, `API-STREAM-001`, `API-PAGE-011`
- `API-RECORD-001`, `API-RECORD-002`, `API-RECORD-003`, `API-RECORD-004`,
  `API-RECORD-005`, `API-RECORD-006`, `API-PREP-004`
- `GEN-BODY-001`, `GEN-RESP-001`, `GEN-RESP-002`, `GEN-PAGE-005`, `GEN-PREP-001`
- `SEC-BODY-003`, `SEC-RESP-009`, `SEC-STREAM-001`, `SEC-RECORD-001`, `SEC-RECORD-002`
- `BOUND-RECORD-001`, `BOUND-RECORD-002`, `BOUND-RECORD-003`
- `EXEC-BODY-006`, `EXEC-BODY-007`, `EXEC-RESP-001`, `EXEC-RESP-002`, `EXEC-STREAM-004`,
  `EXEC-RECORD-001`, `EXEC-RECORD-002`, `EXEC-RECORD-003`, `EXEC-RECORD-004`,
  `EXEC-RECORD-005`
- `FEAT-RECORDS-001`, `FEAT-CSV-001`, `FEAT-NDJSON-001`, `COMPAT-MACRO-005`
- `CONF-BODY-001`, `CONF-RESP-001`, `CONF-RECORD-001`, `CONF-RECORD-002`

Current normative coverage contains 563 active requirements and 11 separately resolvable
superseded requirements, for 574 coverage rows. Historical evidence remains valid only for its
recorded source and paragraph version.

ADR-045, “Generic body and response specifications use sealed core acquisition modes,” is accepted
and supersedes ADR-022. It preserves the valid intent of ADR-007, ADR-013, ADR-019, ADR-021,
ADR-023, ADR-024, ADR-026, ADR-029, and ADR-039. Both architecture owners now describe one body and
response lifecycle, one executor, core-sealed acquisition, integrated records, private native
ownership, and no macro semantic type categories.

## Request-body contract and lifecycle

The stable public specification is exactly:

```rust
pub trait RequestBodySpec: Send + Sync + 'static {
    type Input: Send + 'static;

    fn metadata(
        input: &Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError>;

    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError>;
}
```

`RequestBodyContext`, `LengthHint`, `RequestContentType`, and `RequestBodyMetadata` are exported
through `concord_core::advanced`. Metadata is structural and pre-hook: tests prove it does not
encode, run factories, frame multipart, poll streams, open files, build a native body, or access
URL/header/auth/limiter/transport state.

There is exactly one public managed outgoing value, `RequestBody`. Core derives replayability only
from the produced value. The generic `EncodeBody` adapter and built-in `Text`, `Json<T>`, `Bytes`,
`Stream<M>`, `Multipart`, and direct `RequestBody` specifications all enter the same lifecycle.
`NoContent` is not a request specification and bodyless endpoints expose no body parameter.

Observed execution order is public head preparation, metadata, known metadata limit checks,
before-send hook, one synchronous `prepare`, metadata/produced-body consistency validation,
managed materialization, credential preflight, limiter acquisition, credential finalization,
native debug/authentication/construction, and managed send. Contradictions in kind, exact/lower
length, content-type ownership, generated ownership, body presence, or managed state fail before
protected work. Hook veto prevents `prepare`; hidden Reqwest retries do not rerun `prepare` or body
factory construction; one-shot, reconstructible, reusable bytes, and multipart replayability retain
their exact recovery behavior; the first excess request frame is withheld.

## Response specification and sealed acquisition

The stable public response contract is exactly:

```rust
pub trait ResponseBodySpec: Send + Sync + 'static {
    type Output: Send + 'static;
    type Mode: ResponseMode;

    fn accept() -> Option<&'static str> { None }

    fn validate(
        head: &ResponseHead<'_>,
    ) -> Result<(), ResponseBodyError> { Ok(()) }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError>;
}
```

`ResponseHead` exposes only the logical URL, successful head, and execution metadata. The public
`ResponseMode` trait is sealed; only core implements these exact modes:

| Mode | Acquired body |
|---|---|
| `Buffered` | `Bytes` |
| `Streaming` | `ByteStream` |
| `Discard` | `()` |

Successful status classification occurs first. Known-length mode preflight precedes validation;
validation and record-decoder start precede the first poll; core alone acquires the selected mode;
`from_body` runs only after acquisition is ready; synchronous extension return is checked against
the operation deadline. No application extension receives a native response, replaces status,
changes limits or completion, or defines a fourth mode.

The generic `DecodeBody` adapter receives only complete bounded bytes. Built-in `Text`, `Json<T>`,
`Bytes`, `NoContent`, `Stream<M>`, and `Records<D>` use the one contract. `NoContent` discards
without polling; streaming remains lazy; response media policy is direction-specific through
`accept` and `validate`, while request content-type policy stays in request metadata.

## Universal metadata and ByteStream

There is one private-field metadata wrapper:

```rust
pub struct ApiResponse<T> { /* private */ }
```

It provides `url`, `status`, `version`, `headers`, `metadata`, `value`, and `into_value`. Its
`Debug` is structural and never formats `T`, URL data, or header values. `DecodedResponse<T>` is only
a non-duplicating type alias to `ApiResponse<T>`. `StreamResponse<M>` is removed. Direct await
returns `T`; `.response()` returns `ApiResponse<T>` for buffered, bytes, no-content, byte-stream,
record-stream, custom streaming, and custom discard outputs. Stream opening still completes once as
`StreamOpened`.

`ByteStream` is the sole media-independent application response stream. It owns one bounded lazy
body and contains neither metadata nor a media marker. Known length is preflighted, unknown chunks
remain unpolled at return, cumulative post-decompression counting withholds the first excess chunk,
native read errors become safe `ApiError` values, `Debug` is non-consuming and body-free, and drop
cancels the owned native body. Later polling is outside the endpoint deadline and later errors emit
no second completion.

## Generated, prepared, and pagination integration

The macro AST and normalized model store request body as `Option<syn::Type>` and response as
`syn::Type`. Production macro source contains no `RequestBodyKind`, `ResponseKind`, `CodecKind`, or
`MediaKind`, and no semantic branch for `Json`, `Text`, `Bytes`, `NoContent`, `Stream`, `Multipart`,
`Records`, `Csv`, or `NdJson`. Feature diagnostics arise from core type and trait availability.

Generated body arguments use `B::Input` under `B: RequestBodySpec`; responses use `R::Output` under
`R: ResponseBodySpec`; metadata terminals use `ApiResponse<R::Output>`. Bodyless endpoints use one
core-private empty specification without exposing a body argument. Phase 12 performs the generated contract `12 -> 13`;
core, macros, mismatch fixture, version tests, downstream fixtures,
Rustdoc, and diagnostics all agree on 13 while Phase 11 history remains at 12.

Prepared endpoints now use only `PreparedEndpointBuilder<R = NoContent>` and
`PreparedEndpoint<R = NoContent>`. Generic `body_spec<B>` and `response<S>` select arbitrary public
specifications. `body`, `body_codec`, `response_bytes`, `response_no_content`, and
`response_stream<M>` delegate to those paths. `PreparedBuffered`, `PreparedBytes`,
`PreparedNoContent`, and `PreparedStream` are absent; prepared endpoints have no executor or
pagination path of their own.

Pagination requires the explicit `PageResponseSpec: ResponseBodySpec<Mode = Buffered>` capability
and `R::Output: IntoIterator`. Buffered codecs with iterable output receive the blanket capability;
downstream buffered specifications may opt in. `Bytes`, `NoContent`, `Stream<M>`, and `Records<D>`
do not paginate. Phase 11 pagination lifecycle, recovery, security, page-completion, and aggregate-
completion tests remain passing.

## Integrated records, CSV, and NDJSON

Records live only inside feature-gated `concord_core::records`; there is no workspace records crate
and no records executor. The exact non-default topology is:

| Feature | Dependencies and isolation |
|---|---|
| `records` | Base record traits/errors/streams/batches; no Serde or CSV dependency |
| `csv` | Enables `records`, optional Serde, and exact optional `csv = 1.3.1`; does not enable JSON or NDJSON |
| `ndjson` | Enables `records`, optional Serde and `serde_json`; does not enable general `json` or CSV |

The default and macro dependency graphs contain none of the records/CSV/NDJSON dependencies. The
optional CSV dependency compiles on Rust 1.85 and passes Cargo Deny.

`RecordDecoder` owns `accept`, successful-head `validate`, one-instance `start`, synchronous
`decode`, exact `decode_eof`, and positive finite `max_buffered_bytes`. `Records<D>` selects
`Streaming`; `RecordStream<D>` yields one `Result<D::Record, RecordError>` at a time; and
`RecordBatches<D>` is only a consumer adapter over a positive batch size.

Tests prove chunk-independent framing, repeated decoder calls before another network poll, EOF
semantics, safe body/decode stream items, batching, drop cancellation, an independent staging
limit, rejection without retaining the first excess append, and withholding an oversized framed
record. Built-in CSV and NDJSON staging is exactly 1 MiB.

CSV proof covers required typed headers, invalid/duplicate headers, comma and double-quote dialect,
doubled quotes, quoted delimiters/newlines, LF/CRLF, one-byte network splits, several rows per chunk,
empty physical lines, optional/final missing newline, malformed quoting, typed decode failure,
oversize withholding, batching, body failure, drop, media parameters, and safe formatting.

NDJSON proof covers typed non-empty logical lines, LF/CRLF, one-byte splits, multiple values per
chunk, optional final newline, whitespace-only lines, malformed and incomplete final values,
oversize withholding, batching, body failure, drop, both registered media names with parameters,
and safe formatting without raw JSON or parser-authored text.

A stable-public-only downstream fixture implements custom request, buffered/streaming/discard/page
response, and record-decoder specifications, then compiles and executes generated and prepared
paths without a macro source change, private import, native response, or second runtime.

## Security and prior guarantees

High-entropy request, body, response, stream, CSV, NDJSON, custom-record, header, authenticated URL,
credential, rate-key, and pagination sentinels were exercised across `Display`, `Debug`, recursive
sources, hooks, debug events, completion events, limiter contexts, public Rustdoc, and generated
Rustdoc. Core error reduction excludes application error text, body/record bytes, parser text,
headers, filenames, JSON values, CSV fields, native authenticated URLs, and credentials. Selected
extensions receive only their documented inputs and own any application logging or storage.

The full regression matrix reproves authentication recovery, limiter reacquisition, hidden Reqwest
retry containment, exact request counts, replayability, provider isolation, terminal status bodies,
bounded dangerous raw success, generated/prepared parity, pagination, page and aggregate
completion, cancellation, error precedence, finite configuration, private Reqwest ownership, and
one production executor.

## Validation

Environment: Rust/Cargo 1.85.0, LLVM 19.1.7, cargo-nextest 0.9.140, just 1.46.0,
cargo-deny 0.20.2, PowerShell 7.6.3, Windows PowerShell 5.1.26100.8655, and Git
2.45.1.windows.1 on `x86_64-pc-windows-msvc`.

Every command ran from `F:\projects\concord_api` against
`d1aadcebcad0a65d92c4ed4a42d6b0a9c1f034b2` and exited zero:

| Exact command | Result |
|---|---|
| `just fmt-check` | passed |
| `just check` | passed |
| `just check-all` | passed |
| `just check-no-default` | passed |
| `just clippy` | passed with warnings denied |
| `just clippy-all` | passed with warnings denied |
| `just test` | 558/558 passed |
| `just test-all` | 583/583 passed |
| `just test-no-default` | 1/1 passed; 244 skipped |
| `just doctest` | 2/2 compile-fail doctests passed |
| `just doctest-all` | 4/4 compile-fail doctests passed |
| `just doc` | all four libraries passed with warnings denied |
| `just doc-all` | all four all-feature libraries passed with warnings denied |
| `just deny` | advisories, bans, licenses, and sources passed; two accepted historical duplicate warnings only |
| `just diff-check` | passed |
| `just release` | passed in 117.2 seconds; complete maintained matrix and 11/11 benchmark categories |

The release wrapper also passed the exact default/all/no-default/records/CSV/NDJSON/combined/JSON/
multipart feature matrix, dependency topology, dangerous-feature isolation, Rust 1.85 matrix,
generic specification and record tests, grammar/UI/downstream fixtures, Reqwest assumptions,
security/properties, examples, benchmarks, public/generated Rustdoc, naming, artifact, registry, and
historical-prefix audits. Registry self-tests passed 15 acceptance/rejection classes with 943 parsed
claims. Source mode reported 563 active, 11 superseded, and 1,553 historical rows.

Validation used deterministic barriers, controlled time, explicit counters, owned loopback protocol
servers, no arbitrary sleep oracle, no automatic retry, and no external live application service.

## Evidence and historical preservation

Phase 12 contributes exactly 127 direct and 0 partial evidence rows, one for every Phase 12-owned
active paragraph version. There are no waivers, missing paths, unresolved Phase 12 atoms, or partial
Phase 12 criteria. The catalog contains 1,680 rows: the exact 1,553 Phase 0-11 evidence rows followed
by the 127 append-only Phase 12 rows.

The historical Phase 0-11 owner remains exactly 1,099,185 bytes with SHA-256
`90d31f417690082af865850d6e1ce25331bca16c76f37c9338382b71df85a91f`. Every Phase 0-11 evidence
byte and every `PHASE_0_REPORT.md` through `PHASE_11_REPORT.md` byte remains unchanged. Superseded
IDs remain resolvable through their change records and retain their exact historical evidence and
source identities.

Coverage contains all 563 active and 11 superseded IDs. The validation registry adds 28 Phase 12
rows, including all 16 required maintained commands and the exact focused feature, extension,
security, MSRV, documentation, dependency, example, benchmark, and release-audit recipes. The
historical Phase 10 release requirement, claim, and change-map registries remain historical; Phase
14 owns whole-product regeneration and release-final closure.

## Limitations and next state

Only these authorized future phases remain:

1. Phase 13 — Lexically Scoped Rate Limits — active, `NOT_STARTED`.
2. Phase 14 — Product Completion, Conformance Closure, and Release Finalization — `NOT_STARTED`;
   owns whole-product evidence regeneration and the only permitted restoration of
   `Normative Release Final`.

Phase 12 adds no lexical rate-limit semantics, removes none of the current rate-limit templates or
attachments, performs no release-final promotion, creates no records crate or second runtime, and
implements no Phase 15 work.

`PHASE_12_COMPLETE_WITH_LIMITATIONS`

`NORMATIVE_IMPLEMENTATION_CANDIDATE`
