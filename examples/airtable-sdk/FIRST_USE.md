# Independent first use — Airtable

## Starting point

Application consumer of `concord-sdk-airtable`; initial HEAD
`e0425cc8a09449cb6f12a492a79d47c5348fb387`, with Concord frozen at
`0983a349`. The worktree was clean; neither `tests/first_use.rs` nor this
file existed. Requested runner: Codex gpt-6-astra (xhigh); this session does
not expose an independently verifiable model/effort identifier.

No SDK implementation, existing SDK tests, `FRICTION.md`, or author notes
were read. No other provider SDK supplied the solution. All requests in
the tests will use loopback and `SYNTHETIC_FIRST_USE_NOT_A_CREDENTIAL`.

## Ordered trace

1. First entry point: SDK `README.md`, followed by SDK public rustdoc for
   `Client`, `ListOptions`, `Record`, `RecordPage`, `Response`, `ResponseMeta`,
   `Error`, `ProviderError`, and `RecordPages`. Generated with
   `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo doc --locked -p concord-sdk-airtable --no-deps`;
   successful, without diagnostics.
2. Read repository `README.md`, `docs/CALLER_GUIDE.md`, the metadata recipe in
   `docs/RECIPES.md`, and `crates/concord-pagination/README.md`. Generated
   Concord rustdoc (`--features csv,jsonl`) and pagination rustdoc, both
   with `--locked --no-deps` and the required build environment prefix;
   successful, without diagnostics. Consulted their public error,
   metadata, page, resume, incomplete-result and item-stream APIs.
3. Consulted the official sources below. Read the public types, methods
   and lifecycle documentation in `examples/testserver/src/lib.rs` to use
   the required fake-server harness. This is a harness source read, not
   an Airtable SDK implementation/test intervention. Existing SDK fixture
   files were not used: the README does not identify specific files.
4. Wrote the following predictions before creating the test implementation.
5. Created `tests/first_use.rs` as an empty file, then added seven outcome-A
   tests and the independent fixtures. First required test command compiled
   successfully with no compiler diagnostics, but all seven tests failed
   the same wire assertion: actual `/v0/v0/appFirstUse/Tasks`, expected
   `/v0/appFirstUse/Tasks`.
6. First corrective intervention: changed only the consumer's local client
   helper from `with_base_url(format!("{}/v0", server.url()))` to
   `with_base_url(server.url())`. The recorded request identified the
   problem; no SDK source/test read, native escape or library edit was
   needed. The SDK's word "origin" was literal; my prediction about
   retaining the version in the replacement base was wrong.
7. Re-ran the required command after that single correction: all seven
   outcome-A tests passed. No compiler diagnostics. Continued to outcome B
   using its prediction above, which was already written before any tests.
8. Added seven tests for filtering/selection, empty results, provider error
   documents, invalid page sizes, long URLs and interrupted error bodies.
   The required command compiled without diagnostics and passed all 14
   tests on this first combined run. No additional corrective intervention.
9. Formatted only the new test file with `rustfmt --edition 2024`.
   `rustc --version` confirmed 1.99.1. Checked changed-file scope, whitespace
   and the fixture text attribute; no tracked files were modified, and
   `crates/`, `scripts/`, `docs/`, `checks/` match the frozen commit.
10. Final required command after formatting: **14 passed, 0 failed,
    0 ignored**, without compiler diagnostics. Explicit formatting and
    whitespace checks covered the new test and this trace.

## Outcome A prediction (before implementation)

`Client::new(http, token).with_base_url(loopback + "/v0")`, then
`list_pages(base, table, ListOptions::default(), Resume::Start, cap)`.
I expect the replacement base to include `/v0` (the SDK calls it an
"origin" but gives no concrete local URL example). Construction is lazy.
The first request is `GET /v0/appFirstUse/Tasks`, with bearer auth, JSON
Accept, no body and no optional query. Each subsequent request carries
the preceding opaque `offset` as one percent-encoded query value.

`next_page()` returns `Page<Record, String, ResponseMeta>` values in order,
including an empty page with a continuation. Record fields preserve JSON
types and omitted keys. Only a response without an offset permits
`Ok(None)`; reaching the cap exactly on that page is complete. A cap with
remaining work returns `PagesError::Bounded`, then `Ended`; its resume
position starts a new traversal at the first unread page.

A failed page is a typed `PagesError::Provider`, with its failed offset;
`items().collect_complete()` retains the prior records. A 503 keeps the
HTTP head, optional provider error and raw bytes, without implicit retry.
A stream yields prior records, then an error, then `None`; that final
`None` alone must not be treated as success. A dropped/explicitly stopped
consumer does not establish exhaustion or fetch ahead. A truncated HTTP
body must fail even if the bytes already contain a complete JSON value;
a complete HTTP body containing malformed JSON should instead be Decode.
A present empty offset is the documented `Error::EmptyOffset`.

## Outcome B prediction (before implementation)

`ListOptions { filter_by_formula, fields, view, page_size, max_records }`
passed to `list_page` should emit GET with `filterByFormula`, repeated
`fields[]`, `view`, `pageSize`, and `maxRecords`. These options should also
survive every `list_pages` request together with its offset. Reserved
characters and Unicode must survive one URL decoding; a slash in the
table name must remain inside one encoded path segment.

`list_page` returns `Response<RecordPage>` with values and HTTP metadata;
an absent offset denotes the last page, and an empty records array is a
valid result. Field selection/filtering is done by the fake provider,
not evaluated by the client. A provider validation error is `Error::Http`
with status and the typed object/string error when decodable; a non-JSON
error retains raw bytes. Invalid page sizes (0 and 101) should fail before
I/O. Long formulas remain GET-only: a fake 414 should surface as an HTTP
error with no POST fallback. That 414 response is a fault injection, not
a claim about Airtable's exact rejection document/status.

## Fixture provenance and scope

All new bytes under `fixtures/first_use/` are hand-authored by this
consumer. They are not captured provider responses or byte-for-byte
copies of documented examples. The existing `.gitattributes` rule
`fixtures/** -text` is retained. Synthetic error messages, tokens, record
IDs, field values and response headers are test data. Empty intermediate
pages, empty offsets, bad JSON and interrupted transfers are explicit
robustness probes, not claims about normal Airtable behavior.

Official references consulted on 2026-11-06:

- [Getting started](https://support.airtable.com/articles/6292134965-getting-started-with-airtable-s-web-api):
  v0 records path, opaque offset query, no-offset completion, page size
  maximum and omitted empty fields.
- [URL length limitations](https://support.airtable.com/articles/9428033215-url-length-limitations-for-web-api-requests):
  GET query spelling, repeated `fields[]`, 16,000-character limit and
  provider POST alternative (outside this SDK's stated scope).
- [Filtering](https://support.airtable.com/articles/1941464361-airtable-web-api-using-filterbyformula-or-sort-parameters):
  encoded formula parameter and view/filter composition.
- [Common troubleshooting](https://support.airtable.com/articles/2111090641-Airtable-API:-Common-troubleshooting):
  structured and string error envelopes, 422 `INVALID_REQUEST_UNKNOWN`,
  404 `NOT_FOUND`, 503 `RETRIABLE_ERROR` and optional Retry-After.

SDK public rustdoc supplies the Rust mappings and optional creation-time
field. Tests establish behavior against these local scripts only, not
live-provider conformance.

## Results and friction

Both business outcomes work using only the SDK public API. The final
suite has **14 passing tests**, including multiple fixture cases inside
the malformed-response, provider-error and invalid-page-size tests.
Distinct compiler diagnostics: **none**. Distinct initial test failure:
the duplicated `/v0` prefix described in trace entries 5–6.

Working path: configure `concord::reqwest::Client` (timeout, no redirects,
loopback bypasses proxies), `Client::new`,
`with_base_url(server.url())`, then `list_page` or `list_pages` with
`ListOptions`. Use the SDK's `pagination` re-export for resume positions
and typed traversal errors. `collect_complete` preserves partial records;
`next_page` preserves page metadata. Tests inspect existing operation
identity without inventing operation-name strings.

Concrete friction and reproductions:

- **Local base URL discovery:** no README loopback example says that the
  SDK itself appends `/v0`. The rustdoc word "origin" was correct, but I
  initially supplied a versioned base URL. Reproduce by changing the
  `client` helper to `with_base_url(format!("{}/v0", server.url()))` and
  running `empty_first_page_is_successful_exhaustion`; the wire assertion
  observes `/v0/v0/appFirstUse/Tasks`. Passing the bare origin fixes it.
  This is a consumer setup correction/documentation opportunity, not an
  established Concord defect.
- **Long filters have no SDK route past the URL limit:**
  `long_filter_surfaces_get_rejection_without_a_post_fallback` sends a
  formula over 16,000 characters, observes one GET and receives the fake's
  414 as `Error::Http`. There is no automatic POST fallback, as the README
  explicitly states. An application needing that documented provider
  alternative cannot use this SDK's current GET-only surface for it.
  No native workaround was added to widen the requested outcomes.
- **Failure/completion handling needs the pagination reference:** the
  README names the mechanisms but does not show a compiled error match.
  `failed_page_keeps_partial_records_provider_details_and_retry_position`
  demonstrates `Incomplete.error -> PagesError::Provider.error ->
  Error::Http`, alongside `error.provider()` and `error.meta()` accessors.
  `interrupted_page_stream_yields_error_before_end_even_with_complete_json`
  demonstrates why `None` after an error is insufficient completion
  evidence; `into_inner().finish().outcome` remains `Failed`. These are
  documented behaviors, not newly discovered correctness defects. The
  public pagination README/rustdoc supplied the working handling.

No Airtable SDK `src/` or existing tests were read. No native request
escape, private reach-through, rebuilt execution, handwritten operation
identity, frozen-library edit, existing-test edit, dependency edit or
README edit was required. `FRICTION.md` was not opened.

Reproduce the complete suite from the workspace root:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-airtable --test first_use
```
