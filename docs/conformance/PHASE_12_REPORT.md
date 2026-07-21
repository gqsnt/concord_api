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

- Original Phase 12 starting HEAD: `b66d2914609aec7b767481e09dedd5544b3d9130`
- First focused correction starting HEAD: `d0c7d0d3cfb4d7553b5a456c13be369d0b01bd3c`
- Record-proof correction starting HEAD: `e7e30bc45012792a6532103b5c22e1b9b54303a7`
- Starting branch/tree for the correction: clean `master`
- Phase 12 authority commit: `95a1b9075061184ff2fa5807bb28b16a4d172277`
- Initial Phase 12 source/test commit: `e71ed0671359f443f5aa165391ff56bd10debdf2`
- Prior focused source correction: `d1aadcebcad0a65d92c4ed4a42d6b0a9c1f034b2`
- Prior executable correction commit: `8281c79d98406be7b88a20ea70d84af531e0b757`
- Record lifecycle/security/Rustdoc correction commit:
  `8307d0744363ea7ec8a98d5f5fe68c4429b71c31`
- Final corrected immutable tested source commit: `8307d0744363ea7ec8a98d5f5fe68c4429b71c31`
- Corrected Phase 12 evidence/report/state commit: `6369a5c2b1742359847b780a7eea455c6bbac921`
- Corrected Phase 12 evidence anchor: `d02e2d86e7da3029c439e564b432e97f01353691`
- Compatible anchor-metadata descendant: the final two-file commit titled
  `docs: record corrected phase 12 records anchor`; its own identity is intentionally not
  self-embedded.
- Head policy: `anchor-must-be-an-ancestor`

The immediately superseded tested source `8281c79d98406be7b88a20ea70d84af531e0b757`, evidence state
`21309a579e3df05847cb42b358775215dfdee544`, evidence anchor
`d781081634ebf50fe5a5c5c41d0ae53fb6192081`, and metadata descendant
`e7e30bc45012792a6532103b5c22e1b9b54303a7` remain immutable correction history. Earlier evidence
state `14942ce98c6e959c9cfe4f1fe328ddd95d3d7367` and anchor
`793d6d30b75b7d7dd17831cdf60c4c504da90f7a` likewise remain reachable history.

The current correction closes four remaining proof classes without changing approved semantics:
record opening and exactly-once `StreamOpened` completion; post-return record failure and synchronous
drop behavior; record/response diagnostic and rendered-Rustdoc exclusions; and public downstream
credential, limiter, hook, and controller runtime sufficiency. It adds deterministic probes,
rendered documentation checks, security-registry allocation, isolated records documentation roles,
and targeted release-audit assertions. The tests exposed no product defect. No public contract,
dependency, feature, generated contract, executor, or Phase 13/14 behavior changed.

No branch was created. No remote Git operation, push, pull, fetch, merge, rebase, amend, tag,
publication, deployment, credential access, or history rewrite occurred. Authoritative fixture and
dependency-topology proof used the maintained locked/offline commands and deterministic loopback
servers only.

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

The corrected downstream proof is
`concord_core/tests/generic_spec_extensions.rs::downstream_request_spec_runs_through_facade_explicit_and_prepared_endpoints`.
It observes three managed HTTP requests, exact per-path content types, one metadata call and one
prepare call per request, one server receipt per distinct body sentinel, and declared response-spec
decoding for facade, explicit generated endpoint, and prepared construction. The explicit path
constructs the actual emitted endpoint value and calls `client.request(explicit_endpoint)`; it does
not call the facade. The companion
`downstream_request_spec_hook_veto_prevents_prepare_for_facade_explicit_and_prepared_paths` proves
metadata once, hook observation, prepare zero, credential acquisition/materialization zero, limiter
acquisition zero, request-debug zero, and native sends zero on every path. The companion
`downstream_request_spec_contradictions_fail_before_protected_phases_for_facade_explicit_and_prepared_paths`
proves representative body-kind, exact-length, content-type-ownership, and impossible-state
contradictions after metadata, hook approval, and one prepare but before those same protected
phases. Every returned category is `RequestBody`, and high-entropy body values remain absent from
formatting and recursive sources.

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
The corrected production-source audit additionally verifies arbitrary application-name neutrality,
`Option<syn::Type>` / `syn::Type` storage, and the same generated `RequestBodySpec` bound and
endpoint capability used by facade and explicit values. `DownstreamBody` required no macro source
change; its successful explicit generated-endpoint execution is the runtime half of that proof.

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

The deterministic execution probe
`records_response_validates_starts_unpolled_and_completes_stream_opened_once` observes, immediately
after endpoint await, decoder validation `1`, decoder start `1`, decode `0`, decode-EOF `0`, and
native body polls `0`. It receives one `RecordStream` over one unpolled `ByteStream`, observes one
response-head hook and the required one limiter response observation, and records exactly one fixed
completion with outcome `StreamOpened`. Direct and metadata terminals use this same lifecycle and
do not duplicate completion.

`record_stream_body_and_decoder_failures_are_items_without_second_completion` covers native body
read failure, decoder framing failure, and decoder EOF failure separately. Endpoint await succeeds
first in every case; the later safe `RecordError` is a stream item; the original `StreamOpened`
record remains unchanged; and no second completion appears. The two synchronous-drop probes prove
that dropping `RecordStream<D>` immediately drops decoder state and the owned byte/native body, and
that dropping `RecordBatches<D>` immediately drops its owned record stream. Drop requires no task,
causes no later poll, and emits no completion.

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

Rendered documentation is tested independently from source comments. The public record Rustdoc
probe renders `records`, `csv`, `ndjson`, and `csv,ndjson` in isolated target directories and checks
feature visibility, structural private fields, value-free debug documentation, stable public paths,
and the absence of native response, transport, executor, staging, or decoder-storage escape. The
generated fixture renders downstream `Records<ExtensionRecordDecoder>`, CSV, and NDJSON endpoints
and checks the same stable-path and value-channel boundary. Rust 1.85 documentation builds passed
for each of the four isolated feature combinations; records-only documentation has no Serde/CSV
claim or dependency, and CSV/NDJSON remain mutually and general-JSON isolated.

## Security and prior guarantees

High-entropy request, body, response, stream, CSV, NDJSON, custom-record, header, authenticated URL,
credential, rate-key, and pagination sentinels were exercised across `Display`, `Debug`, recursive
sources, hooks, debug events, completion events, limiter contexts, public Rustdoc, and generated
Rustdoc. Core error reduction excludes application error text, body/record bytes, parser text,
headers, filenames, JSON values, CSV fields, native authenticated URLs, and credentials. Selected
extensions receive only their documented inputs and own any application logging or storage.

The new executing probes (not merely the registry audit) cover raw record bytes, CSV headers and
fields, NDJSON values, downstream decoder state, parser-authored text, response headers, and logical
URL continuation/query data. They inspect `ApiError` display/debug/sources, hooks, request and
response debug, limiter request/observation, completion, `ApiResponse<T>`, `ByteStream`,
`RecordStream<D>`, `RecordBatches<D>`, and `RecordError`. A companion response probe composes
buffered bytes, stream chunks, records, decoded/custom output, and parser text across those same
automatic surfaces. The security registry now has distinct `record_data_runtime`,
`record_data_rustdoc`, and generated-record-Rustdoc allocations; maintained security selection
executes each semantic probe rather than treating registry presence as proof.

The public-only downstream workflow map is:

| Workflow | Exact runtime owner |
|---|---|
| request, body, codec, response, prepared | `concord_core/tests/generic_spec_extensions.rs` |
| record | `concord_core/tests/records_extensions.rs` |
| page response, controller | generated pagination plus `extension_certification/tests/pagination_runtime.rs` |
| credential, limiter, hook | `extension_certification/tests/extension_workflows_runtime.rs` |

The standalone credential/limiter/hook probe uses a public `DirectExtensionProvider`, public
`RateLimiter`, public `RuntimeHooks`, an ordinary generated request, an ephemeral loopback TLS
application/proxy path, exact counters, and high-entropy sentinels. Together with the fixture's
public/private escape audit, it proves compile and runtime sufficiency without native response
access, macro modification, private imports, or external provider/application service access.

The full regression matrix reproves authentication recovery, limiter reacquisition, hidden Reqwest
retry containment, exact request counts, replayability, provider isolation, terminal status bodies,
bounded dangerous raw success, generated/prepared parity, pagination, page and aggregate
completion, cancellation, error precedence, finite configuration, private Reqwest ownership, and
one production executor.

## Validation

Environment: Rust/Cargo 1.85.0, LLVM 19.1.7, cargo-nextest 0.9.140, just 1.46.0,
cargo-deny 0.20.2, PowerShell 7.6.3, and Git
2.45.1.windows.1 on `x86_64-pc-windows-msvc`.

Every command ran from `F:\projects\concord_api` against
`8307d0744363ea7ec8a98d5f5fe68c4429b71c31` and exited zero. Every filtered Nextest run was
preceded by its matching `cargo nextest list ...` command; the inventory selected the stated
fully-qualified tests and no zero-test selection was accepted.

The exact corrected request-specification and macro commands were:

```text
cargo nextest run --profile ci -p concord_core --test generic_spec_extensions -E 'test(=downstream_request_spec_runs_through_facade_explicit_and_prepared_endpoints) | test(=downstream_request_spec_hook_veto_prevents_prepare_for_facade_explicit_and_prepared_paths) | test(=downstream_request_spec_contradictions_fail_before_protected_phases_for_facade_explicit_and_prepared_paths)'
cargo nextest run --profile ci -p concord_macros --test macro_type_neutrality -E 'test(=production_macro_model_is_type_shaped_and_semantically_neutral)'
```

They selected and passed 3/3 and 1/1 tests. The exact prior-guarantee correction commands were:

```text
cargo nextest run --profile ci -p concord_core --all-features --lib --test reqwest_contract -E 'test(/hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle$/) | test(=hidden_retry_does_not_reenter_concord_boundaries) | test(/reconstructible_factory_runs_once_per_concord_attempt_and_no_third_attempt_occurs$/)'
cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(/real_tls_provider_http_remains_outside_application_limiter_lifecycle$/) | test(/paginated_provider_http_never_enters_page_lifecycle_or_limiter_work$/) | test(/provider_http_data_stays_outside_application_hooks_debug_and_channel$/)'
cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(/factory_content_type_is_captured_before_metadata_without_materialization$/) | test(/multipart_replayability_and_factory_freshness_are_internal$/) | test(/factory_materialization_and_explicit_reconstruction_are_attempt_scoped$/) | test(/request_stream_and_factory_remain_lazy_through_hook_veto$/) | test(/body_factory_is_created_once_after_approval_and_failure_is_pre_native$/) | test(/reconstructible_factory_runs_once_per_concord_attempt_and_no_third_attempt_occurs$/) | test(/hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle$/)'
cargo nextest run --profile ci -p concord_core --all-features --lib --test raw_response -E 'test(=raw_success_skips_decoder_media_validation_and_preserves_exact_metadata) | test(=raw_success_uses_known_length_preflight_and_never_returns_partial_excess) | test(=non_success_remains_the_ordinary_typed_status_error) | test(/raw_response_reuses_limiter_hooks_observation_and_success_completion$/) | test(/raw_response_contains_hidden_reqwest_retry_inside_one_logical_attempt$/) | test(/raw_response_keeps_the_fixed_deadline_and_cancellation_contract$/) | test(/raw_response_preserves_authentication_recovery_and_logical_native_url_separation$/)'
```

Those inventories selected 3 hidden-retry, 3 provider-isolation, 7 factory, and 7 raw-response
tests, all passing. They prove logical-attempt-scoped hook/body/auth/limiter counts, no native-retry
prepare/factory/credential/limiter re-entry, provider HTTP exclusion from every application
lifecycle/channel, metadata without factory materialization, attempt-scoped fresh factory streams,
one permitted recovery reconstruction and no third attempt, pre-native safe factory failure, and
raw decoder bypass/status/bounds/logical URL/hooks/limiter/recovery/retry/deadline/cancellation/
completion parity.

The principal response and records commands were:

```text
cargo nextest run --profile ci -p concord_core --test generic_spec_extensions -E 'test(=downstream_request_and_all_sealed_response_modes_work_without_macro_changes) | test(=custom_response_modes_observe_status_preflight_validation_and_acquisition_order) | test(=synchronous_response_extension_return_is_checked_against_the_operation_deadline) | test(=application_response_errors_and_complete_body_bytes_are_sanitized_by_core)'
cargo nextest run --profile ci -p concord_core --all-features --lib --test records_extensions -E 'test(/records::csv::tests::/) | test(/records::ndjson::tests::/) | test(/records::stream::tests::/) | test(=downstream_decoder_streams_and_batches_through_generated_public_contracts) | test(=csv_records_are_typed_chunk_independent_bounded_and_safe) | test(=ndjson_records_are_typed_chunk_independent_bounded_and_safe)'
```

They selected and passed 4/4 response-mode and 16/16 record tests. The records-only selectors passed
5/5 under `records`, 10/10 under `csv`, 9/9 under `ndjson`, and 14/14 under `csv,ndjson`; the exact
topology command `& .\scripts\audit-record-feature-topology.ps1` passed, as did
`cargo nextest run --profile ci -p concord_macros --test dependency_policy` (4/4) and `just deny`.

The exact record lifecycle, diagnostic, Rustdoc, and downstream workflow commands were:

```text
cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(/records_response_validates_starts_unpolled_and_completes_stream_opened_once$/) | test(/record_stream_body_and_decoder_failures_are_items_without_second_completion$/) | test(/dropping_record_stream_synchronously_drops_decoder_staging_and_byte_stream$/) | test(/dropping_record_batches_synchronously_drops_record_stream_and_byte_stream$/)' --no-tests fail
cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(/record_payloads_never_enter_hooks_debug_limiter_completion_or_sources$/) | test(/response_payloads_never_enter_hooks_debug_limiter_completion_or_wrapper_debug$/)' --no-tests fail
cargo nextest run --profile ci -p concord_core --all-features --test public_rustdoc -E 'test(=record_surfaces_are_structural_private_and_value_free_in_rendered_rustdoc)' --no-tests fail
cargo nextest run --profile ci -p concord_macros --test compile -E 'test(=generated_records_rustdoc_uses_stable_paths_and_exposes_no_record_value_channel)' --no-tests fail
cargo nextest run --profile ci -p concord_macros --test compile -E 'test(=complete_extension_fixture_compiles_through_public_namespaces_for_every_owned_feature_set) | test(=complete_extension_fixture_has_no_private_native_or_integration_escape) | test(=downstream_pagination_runtime_executes_generated_construction_and_binding) | test(=downstream_credential_limiter_and_hooks_execute_through_public_runtime)'
```

Their inventories selected and their runs passed 4/4 lifecycle, 2/2 automatic-surface, 1/1 public
records Rustdoc, 1/1 generated records Rustdoc, and 4/4 public-only downstream fixture tests. The
last command maps compile plus runtime coverage across request, response, page response, record,
codec, body, prepared, credential, limiter, hook, and controller when combined with the exact core
request/response/record selectors named above.

The isolated Rust 1.85 rendered-documentation commands all passed:

```text
cargo +1.85.0 doc --target-dir target/rustdoc-records-only -p concord_core --no-default-features --features records --no-deps
cargo +1.85.0 doc --target-dir target/rustdoc-csv-only -p concord_core --no-default-features --features csv --no-deps
cargo +1.85.0 doc --target-dir target/rustdoc-ndjson-only -p concord_core --no-default-features --features ndjson --no-deps
cargo +1.85.0 doc --target-dir target/rustdoc-csv-ndjson -p concord_core --no-default-features --features csv,ndjson --no-deps
```

The corrected conformance rows use exact principal artifacts and commands rather than `just release`
alone:

- positive generated/downstream compilation uses `cargo nextest run --profile ci -p concord_macros
  --test compile`, `cargo check --target-dir target/fixture-contracts --manifest-path
  concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification
  --all-features --offline --locked`, and the 30/30 generated neutrality/pagination/prepared/surface
  command;
- invalid specifications, sealed modes, non-`Send` input, disabled JSON/multipart, pagination
  exclusions, and contract-13 mismatch use `cargo nextest run --profile ci -p concord_core --test
  ui`, its `--all-features` counterpart, `cargo nextest run --profile ci -p concord_macros --test
  ui`, and the macro compile fixtures;
- public-only extensions use the focused three-test extension fixture command plus the corrected
  request parity, four-mode response, records, and generated pagination commands;
- the maintained high-entropy security selectors passed their exact nonzero inventories (including
  10/10, 13/13, 6/6, 8/8, 1/1, and 2/2 focused groups) across request, response, stream, records,
  raw, credential, rate, pagination, automatic-surface, and documentation probes;
- public/generated documentation and examples use `just doctest`, `just doctest-all`, `just doc`,
  `just doc-all`, `cargo nextest run --profile ci -p concord_core --all-features --test
  public_rustdoc`, the focused generated-Rustdoc compile test, both examples checks, and the 1/1
  canonical examples test.

The exact Rust 1.85 proof ran `rustc +1.85.0 --version --verbose` followed by `cargo +1.85.0 check`
for core no-default, core all-features, `records`, `csv`, `ndjson`, `csv,ndjson`, macros all targets,
the renamed-core locked/offline fixture, the all-feature public extension locked/offline fixture,
all-feature examples, and the no-default JSON dangerous-raw example. Every command passed with
rustc 1.85.0 and LLVM 19.1.7. The validation registry records each full command independently.

| Exact command | Result |
|---|---|
| `just fmt-check` | passed |
| `just check` | passed |
| `just check-all` | passed |
| `just check-no-default` | passed |
| `just clippy` | passed with warnings denied |
| `just clippy-all` | passed with warnings denied |
| `just test` | 570/570 passed |
| `just test-all` | 595/595 passed |
| `just test-no-default` | 1/1 passed; 244 skipped |
| `just doctest` | 2/2 compile-fail doctests passed |
| `just doctest-all` | 4/4 compile-fail doctests passed |
| `just doc` | all four libraries passed with warnings denied |
| `just doc-all` | all four all-feature libraries passed with warnings denied |
| `just deny` | advisories, bans, licenses, and sources passed; two accepted historical duplicate warnings only |
| `just diff-check` | passed |
| `just release` | passed in 216.6 seconds; complete maintained matrix and 11/11 benchmark categories |

The release wrapper also passed the exact default/all/no-default/records/CSV/NDJSON/combined/JSON/
multipart feature matrix, dependency topology, dangerous-feature isolation, Rust 1.85 matrix,
generic specification and record tests, grammar/UI/downstream fixtures, Reqwest assumptions,
security/properties, examples, benchmarks, public/generated Rustdoc, naming, artifact, registry, and
historical-prefix audits. Registry self-tests passed 16 acceptance/rejection classes with 943 parsed
claims. Source mode reported 563 active, 11 superseded, 1,553 historical rows, and all 127 inherited
Phase 12 rows while the corrected evidence was intentionally not yet assessed.

Validation used deterministic barriers, controlled time, explicit counters, owned loopback protocol
servers, no arbitrary sleep oracle, no automatic retry, and no external live application service.

## Evidence and historical preservation

Phase 12 contributes exactly 127 direct and 0 partial evidence rows, one for every Phase 12-owned
active paragraph version. There are no waivers, missing paths, unresolved Phase 12 atoms, or partial
Phase 12 criteria. The catalog contains 1,680 rows: the exact 1,553 Phase 0-11 evidence rows followed
by the 127 append-only Phase 12 rows.

Each corrected Phase 12 row now embeds the complete active normative paragraph as its claim scope,
names principal source/test/fixture files rather than broad crate roots, separates multiple exact
commands with the audited ` ;; ` delimiter, and binds every command to an authoritative passing
validation row for `8307d0744363ea7ec8a98d5f5fe68c4429b71c31`. Human review decomposed every
paragraph conjunction,
modal clause, and enumerated member before assigning paths and commands. The ignored audit contains
one reviewed section for each of the 127 rows. In addition to the prior hidden-retry, provider,
factory, raw-response, compile, downstream, security, MSRV, documentation, body, response, and
records corrections, the six remaining high-risk rows now have these exact allocations:

- `P12-SEC-RECORD-001`: executing automatic-surface and record-error probes plus separate public and
  generated rendered-Rustdoc commands;
- `P12-SEC-RESP-003`: buffered/custom, byte-stream, record, decoded output, parser text, hooks,
  debug, limiter, completion, sources, and wrapper-debug execution;
- `P12-EXEC-RECORD-001`: validate/start once, zero pre-return polls, one returned stream, and one
  `StreamOpened` completion in the deterministic executor;
- `P12-EXEC-RECORD-005`: synchronous stream/batch drop plus later body, framing, and EOF failure
  items with an observed unchanged single completion;
- `P12-CONF-RECORD-002`: Cargo graph, isolated compile/test/Rustdoc, dependency policy, Rust 1.85,
  locked downstream fixture, and Cargo Deny;
- `P12-CONF-DOWNSTREAM-002`: an explicit workflow-to-command map covering every stable public
  request, response, page, record, codec, body, prepared, credential, limiter, hook, and controller
  workflow at compile time and runtime where applicable.

The historical Phase 0-11 owner remains exactly 1,099,185 bytes with SHA-256
`90d31f417690082af865850d6e1ce25331bca16c76f37c9338382b71df85a91f`. Every Phase 0-11 evidence
byte and every `PHASE_0_REPORT.md` through `PHASE_11_REPORT.md` byte remains unchanged. Superseded
IDs remain resolvable through their change records and retain their exact historical evidence and
source identities.

Coverage contains all 563 active and 11 superseded IDs. The validation registry contains 179 rows:
71 preserved historical rows plus 108 corrected Phase 12 rows. Those Phase 12 rows include all 16
required maintained commands, every passing focused execution command cited by evidence, and every
matching Nextest inventory used to establish a nonzero exact selection. The
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
