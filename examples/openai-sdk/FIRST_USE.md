# SDK08: independent first use

Baseline: Concord `e04236ca`, SDK author commit `8a659204`. Requested runner:
Codex gpt-6-astra at xhigh; this consumer session has no tool exposing its actual
model/effort setting, so that identity is not independently verified here.

## Outcome A prediction — recorded before implementation

Starting at the README's `OpenAi::new` and `with_base_url`, I expect
`create_response(ResponseRequest::new(...))` to POST JSON to `/v1/responses`
with bearer authentication and no enabled streaming flag. Its return value
should contain the response object plus HTTP status, headers and declared
operation identity. A 2xx response with `status: failed` is still data; an HTTP
error should be `Error::Provider`, with a typed error document when readable.

`stream_response` should POST the same input with `stream: true` and return an
incremental SSE reader. I expect created, text delta and completed events;
`Completed` followed by `None` means success. A clean HTTP EOF before completion,
malformed event, `response.failed`, provider `error`, or transport interruption
must not be mistaken for completion. Already delivered text remains usable.
An incomplete response should not be successful either; its exact SDK mapping
is not documented. I will check chunk boundaries, UTF-8, CRLF and multiline data.

## Ordered factual trace

1. First entry point: `examples/openai-sdk/README.md`, then generated SDK rustdoc.
   Read package manifest and `.gitattributes`; the latter already has
   `fixtures/** -text`. No pre-existing first-use test or report was present.
2. Read Concord `README.md`, `docs/CALLER_GUIDE.md` and generated `Operation`
   rustdoc. Both requested `cargo doc` commands succeeded with the mandated
   build-cache/job prefix. The SDK rustdoc supplies method signatures and enum
   variants; most methods and error variants have no further semantic docs.
3. Verified provider shapes against the official sources linked below. The web
   reader rejected their `.md` variants and the guide's full streaming-reference
   link; the HTML guides and Files reference were readable. A local HTML-text
   helper could not import `bs4`; Python's standard library sufficed instead.
4. Read the shared `examples/testserver/src/lib.rs` public API to construct the
   independent fake and observe wire requests. This is test harness inspection,
   not an SDK source intervention. No other provider SDK supplied a solution.
5. Before implementing A, created this prediction and an empty
   `tests/first_use.rs`. All consumer fixtures will be new files under
   `fixtures/first-use/`; the SDK README does not point to reusable fixtures.
6. Implemented A using the README and public signatures. First test command:
   10 passed, no compiler diagnostics, no assertion failures. Confirmed ordinary
   and streaming wire headers/body, metadata, 2xx domain failure, HTTP error
   documents, malformed JSON, chunked SSE, early EOF, typed stream failures and
   cancelling a pending read. `response.incomplete` becomes `Other`; its payload
   is unavailable, and subsequent EOF is a generic stream error.
7. Recorded the B prediction below, then implemented the upload/create/poll/read
   path. The next compile and test run passed all 18 tests without diagnostics
   or assertion failures. No SDK implementation inspection was necessary.
8. Opened the current streaming-events reference to confirm the incomplete
   event's provider-defined payload. Adjusted synthetic batch counts to match
   each lifecycle state, and used `batch_expired` only for the expired fixture
   case. Formatted only the new test file and reran the requested command:
   18 passed, 0 failed, 0 ignored. The standalone rustfmt check passed.
9. Verified Git attributes for all 11 new fixture files: `text: unset` for each.
   Git status contains only the new report, consumer test and fixture directory;
   no tracked files changed. Existing SDK/library source, README, tests and
   `.gitattributes` remain unchanged.

## Outcome B prediction — recorded before implementation

`batch_jsonl` should encode unique application IDs into newline-delimited
`POST /v1/responses` envelopes. `upload_batch_file` should send multipart
`purpose=batch` and the exact file bytes to `/v1/files`, returning file metadata.
`BatchRequest::responses(file.id)` should create a batch with that file ID,
`endpoint=/v1/responses` and `completion_window=24h` at `/v1/batches`.

`poll_batch` should repeatedly GET `/v1/batches/{id}`, stop on a terminal state,
and return `PollLimit(last_batch)` on an exhausted attempt cap. Failed, expired
and cancelled jobs are data; HTTP errors are errors. An unknown status must
not count as success. `batch_results` should GET each available file's content,
correlate out-of-order rows by `custom_id`, retain both successes and item errors,
and expose malformed/duplicate rows and independent download failures without
discarding valid results. Content limits should be enforced per file.

I will test exact upload bytes, successful and unsuccessful terminal states,
attempt exhaustion, a failed status request, both directions of partial file
failure, malformed/duplicate rows, and input validation before any HTTP write.
The public result type has string row problems and no explicit completeness
flag; the application must inspect every diagnostic collection and compare IDs
with its submitted IDs before considering reconciliation complete.

## Fixture provenance

All fixtures in `fixtures/first-use/` and inline JSON response documents in the
consumer test are hand-authored synthetic examples, not captures or verbatim
vendor examples. IDs, model names and authentication are synthetic. Negative
fixtures deliberately violate framing, JSON syntax, uniqueness or transport
completion. These are offline consumer tests, not provider conformance evidence.

Official documentation consulted on 2026-11-06:

- [Streaming Responses](https://developers.openai.com/api/docs/guides/streaming-responses):
  streaming flag, SSE transport and event types.
- [Files create](https://developers.openai.com/api/reference/resources/files/methods/create):
  multipart upload fields and returned file metadata.
- [Batch guide](https://developers.openai.com/api/docs/guides/batch): JSONL envelopes,
  upload purpose, create/retrieve requests, terminal states and correlation IDs.
- [Streaming events reference](https://developers.openai.com/api/reference/resources/responses/streaming-events):
  confirmed `response.failed` and `response.incomplete`, including the latter's
  `incomplete_details.reason` field.

Fixture inventory: `completed.sse` uses CRLF, a comment, multiline data and UTF-8;
the other five SSE fixtures use LF and represent early EOF, a failed response,
a provider error event, malformed event JSON, and an incomplete response.
`input.jsonl`, `output.jsonl`, `errors.jsonl` and `expired.jsonl` model the batch
exchange; `rows-with-problems.jsonl` deliberately contains invalid JSON and a
duplicate ID, followed by a valid row. All JSONL fixtures end with LF. HTTP JSON
objects that do not depend on byte layout are constructed in the fake itself.

## Interventions and friction

First SDK intervention: **none**. No SDK source, existing tests or `FRICTION.md`
was read. No native escape, private reach-through, rebuilt execution or
handwritten operation identity was needed. The reqwest facade is used solely to
configure the transport passed to `OpenAi`; every provider request uses the SDK.
Distinct Rust compiler diagnostics: **none** across all three test runs.

Concrete observations, with reproductions in `tests/first_use.rs`:

- **Incomplete response details disappear.**
  `incomplete_event_is_not_success_but_its_reason_is_not_exposed` serves a delta,
  then `response.incomplete` with `reason=max_output_tokens`. The caller gets
  `Event::Other`, then `Error::Stream` at EOF, and cannot distinguish the provider
  reason from a truncated connection using this high-level API. Other event
  payloads being omitted is documented, but incomplete termination deserves a
  distinct representation. `ResponseObject` also has no field for incomplete
  details. The test retains the working failure-detection path; no parser escape
  or library patch was introduced.
- **Polling context is split or discarded on failure.**
  `polling_limit_returns_the_last_observation_including_unknown_status` proves
  the attempt cap preserves a `Batch`, but `PollLimit(Box<Batch>)` has no HTTP
  headers or operation metadata. Unknown status strings become `Unknown` without
  their spelling. `failed_poll_request_returns_provider_error_without_retrying_to_completion`
  first observes progress, then gets a typed 503; that error retains HTTP
  metadata but not the earlier batch. Applications needing both checkpoint and
  diagnostic metadata would have to manage polling with `batch` themselves.
  No such workaround was necessary for these bounded business outcomes.
- **Batch reconciliation remains application work.**
  `either_result_file_can_fail_without_discarding_the_other_file` demonstrates
  that `batch_results(...).await` is `Ok` even when a file fails; callers must
  inspect `file_errors`. `malformed_and_duplicate_rows_are_reported_while_later_rows_survive`
  keeps later valid rows but returns row problems as strings. There is no typed
  row-problem enum or completeness flag. The successful workflow explicitly
  compares returned IDs with the submitted set and checks both diagnostics
  collections. These semantics are documented and preserve useful progress.

The SDK README was sufficient for the working sequence; rustdoc supplied the
exact types for matching failures. In particular, the public `Event::Failed`
and `Event::Error` variants exist, but the reader signals those events through
`Error::StreamFailed` and `Error::StreamEventError`. The README describes errors;
variant-level rustdoc examples would make this distinction easier to discover.

## Working path and validation

A: `OpenAi::new(...).with_base_url(...)` → `create_response`, or
`stream_response` → repeated `next` → explicit completed event. B:
`batch_jsonl` → `upload_batch_file` → `file` metadata → `create_batch` →
`poll_batch` → `batch_results`; `content` also verifies exact raw file bytes and
an inclusive byte limit. No pagination surface is involved; a failed status
fetch and failed independent result downloads cover partial HTTP failure here.

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-openai --test first_use
```

Final result: **18 passed; 0 failed; 0 ignored**. Append a test name above to the
same command to reproduce an individual observation. This is loopback fixture
verification only; no credentials or live provider calls were used. All server
instances are shut down, and the stalled stream is explicitly dropped.
