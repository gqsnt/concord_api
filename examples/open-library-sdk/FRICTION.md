# SDK21 Concord friction

Evidence is at frozen baseline `56a16ee3`, with generated client → scope → endpoint navigation.
This is **example-assisted** work. The full direct/excerpt example-file ledger and what each
contributed are in [PROVENANCE.md](PROVENANCE.md#files-and-examples-consulted). No shared code
was edited; the proposals below describe possible changes only. No compile failure, macro bug
or warning was encountered in the initial build, tests or Clippy gate.

## F1 — bounded JSON/error interpretation still needs a send policy

Observable evidence: `src/policy.rs` implements one generic `SendWith<T>` that calls
`Endpoint::prepare`, `receive`, `document::<Json, Value>(ERROR_BODY_LIMIT)` on non-2xx and
`decode_limited(RESPONSE_BODY_LIMIT)` on success. The generated declaration and ordinary
`send()` compiled on the first attempt. `oversize_success_is_limited_for_content_length_and_chunked_bodies`
and `http_status_survives_every_error_document_failure` exercise this policy's two paths.

Handwritten work: the success/error branch, two limits and construction of the SDK HTTP error.
Generic mechanics: bounded reading and preserving an HTTP failure independently from document
failure. Provider/SDK semantics: selected limits, whether JSON is strict, opaque error JSON,
and no retries. The baseline's `Received::document` and error heads already remove native body
reads, repeated endpoint labels and most manual metadata copying.

Smallest proposed change: an optional external bounded-response policy/helper accepting the
success cap, error format/document type and HTTP-error constructor. It could remove this
repeated branch without a new DSL keyword or placing provider schemas in core. This SDK's
short policy is sufficient for the frozen baseline; no native escape was needed.

## F2 — generated send policy does not follow a prepared request

Observable evidence: `prepared_send_has_concord_status_semantics_and_bypasses_sdk_policy`
receives a Concord `Status` error from `prepare()?.send()` for the same 404 which ordinary
`send()` maps to `Error::Http`. `ReadPolicy` rustdoc and README explicitly explain the bypass.
The policy is chosen by generated `send()`, while `Prepared` retains only the declared decoder.

Handwritten work: public documentation of the two execution contracts and one protocol control.
This is a generic boundary/discoverability cost, unrelated to Open Library's routes. It can
matter for byte caps even without signing or credentials.

Smallest proposed change: generated `prepare()` rustdoc should name the client's selected
policy and state that `Prepared::send()` bypasses it. Preserve the lower-level API; a separate
opt-in policy-preserving preparation API would need a broader compatibility design and is not
required by this slice.

The generated HTML rustdoc for `OpenLibrarySearchWorks::prepare` was inspected after the
documentation gate: it says construction sends nothing and lists Build failures, but does not
name `ReadPolicy` or mention that sending the returned Prepared bypasses it.

## F3 — opaque HTTP diagnostics need a manually redacted wrapper

Observable evidence: `src/error.rs` defines `HttpError` and its explicit Debug/Display, stores
`ResponseMeta` plus `Result<Value, concord::Error>`, and exposes data only through named
accessors. `all_non_success_statuses_keep_status_and_opaque_json_without_retry` checks status,
header access, document access and safe formatting; other protocol tests check malformed
JSON, body failures, cookies and query secret sentinels. Concord error heads avoid storing a
second metadata copy for success-body failures. The Author Guide's Frankfurter excerpt was
the consulted pattern for keeping status authoritative.

Handwritten work: error variants/conversion, redacted HTTP wrapper, status accessor and no
automatic source chain. Generic mechanics: safe storage/formatting of metadata and any-status
document outcomes. Provider/SDK semantics: there is no guaranteed error envelope in the cited
docs, so a typed Open Library error-code enum would be unjustified.

Smallest proposed change: an external `HttpDocumentError<D>` with safe formatting, metadata and
explicit unredacted document access would remove most wrapper code. Keep the SDK's error enum
and ID errors in the SDK. Core's existing redaction was sufficient; no shared bug was observed.

## F4 — canonical IDs and sparse provider models remain SDK work

Observable evidence: source search examples show both `OL27448W` and `/works/…` keys;
`models.rs` normalizes only the known prefix into a validated one-segment WorkId/AuthorId.
`wire_shapes_are_endpoint_specific_and_queries_are_encoded` proves the resulting `.json`
targets without double-encoding `/works/`. Identifier protocol cases reject URLs/slugs/query
injection. Both workflow tests safely decode sparse records; missing collection envelopes
remain decoding failures. The Recipes multi-segment example was considered, but no native
path rewriting is necessary after normalization.

Handwritten work: two ID types, explicit parsing, `hit.id()` conversion, optional metadata,
the two count aliases and endpoint-specific pagination inputs. Generic mechanics: ordinary
Concord encoding/generated setters already handle the final request. Provider semantics:
resource prefixes/letters, page starts at one for work search, author works uses offset, and
the sparse projection are SDK responsibilities. `NonZeroU32` costs an explicit constructor at
call sites; it prevents zero values without a runtime Concord validation layer.

Smallest change that would remove the need for a native workaround is already present:
single-segment formatting plus SDK normalization. No core change should absorb OLID or field
rules. A guide recipe for canonical resource-key normalization (contrasted with real
multi-segment keys) could reduce discovery cost; it would not eliminate provider modelling.

## Retained simple path and capability decisions

The declaration supplies all four routes, request encoding, Accept suggestions, hierarchy,
endpoint identity and `q` redaction. Shared `concord-codecs` JSON supplies media/syntax checks;
`Received` supplies bounded reads and head retention. No custom codec, native request edit,
authentication adapter or wrapper method per endpoint was necessary. Public data requires no
credential flow. Explicit page/batch selection needs no traversal state, budget/checkpoint
adapter or `concord-pagination`; provider counts never drive an unbounded loop.
No automatic replay/backoff is part of this bounded contract, so `concord-retry` is omitted.
429 metadata remains available for application scheduling. There is no claim that provider
rate limits have been experimentally established.

## Example consultation index

Direct: `examples/testserver/src/lib.rs` (wire harness, reply modes, Script and cleanup) and
`examples/testserver/Cargo.toml` (package integration).
Required-guide excerpts only:

- `examples/quickstart/src/lib.rs`: generated API and bounded response usage.
- `examples/quickstart/src/main.rs`: local stand-in workflow.
- `examples/shapes/tests/wire.rs`: caller-owned transport; no subdomain setup adopted.
- `examples/native/src/native_edit.rs`: native exits considered; no edits adopted.
- `examples/compat/src/lib.rs`: resource-segment workaround considered; SDK normalization suffices.
- `examples/openai-sdk/src/lib.rs`: multipart excluded; no implementation adopted.
- `examples/google-drive-sdk/src/upload.rs`: multipart excluded; no implementation adopted.
- `examples/frankfurter-sdk/src/lib.rs`: authoritative HTTP status plus fallible error document.

No existing provider SDK source file was read directly. See PROVENANCE.md for the precise
access distinction and the additional non-example guide snippets.
