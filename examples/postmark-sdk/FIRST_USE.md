# SDK26 Postmark — fresh consumer

Date: 2026-11-07. Baseline: `4f29a81730d9e1bc29b9b8b356aea305c9e12fd2`.
Live provider calls: **NOT_RUN**. All SDK calls target `TestServer` loopback URLs
with an invented server token. This record contains observations and decisions,
not private reasoning transcripts.

## Predictions

Recorded before creating `tests/first_use.rs` or making any SDK call.

### Workflow 1: submit a receipt, then inspect it

Construct `Postmark::new(http_client_builder().timeout(...).build()?,
PostmarkConfig { token })`, replace the base with the fixture's URL, and enter
`api.outbound()`. Call `outbound.send_email(Email { ... }).send().await`, then
`outbound.details(&acceptance.message_id).send().await`.

Expected wire: `POST /email`, no query, JSON PascalCase fields `From`, `To`,
`Subject`, `TextBody`, and `MessageStream`; I expect absent `HtmlBody` to be
omitted (the README does not specify omission versus null). Headers include
`Content-Type: application/json`, `Accept: application/json`, and exactly one
`X-Postmark-Server-Token`. The details request is bodyless
`GET /messages/outbound/{returned-ID}/details`, no query, with the same token
and JSON Accept header. No account token or bearer Authorization is needed.

Expected result: `Result<Acceptance, Error>` with recipient, submission time,
message ID, provider error code zero and response message, followed by
`Result<Message, Error>` with ID, activity status and remaining provider fields
in `fields`. Neither acceptance nor activity status alone establishes delivery;
the fixture will return `Queued` with no delivery event. Ordinary success returns
the decoded value without HTTP response headers/status. Details and provider
metadata such as message events remain in the JSON field map.

Expected failure: non-2xx status becomes `Error::Api`, with status, optional
bounded `ErrorDocument`, and integer `Retry-After` seconds. Sender validation
errors should retain the provider code. Invalid JSON on non-2xx should preserve
status but have no document. A 2xx malformed, wrong-media, incomplete or stalled
send response becomes `Error::UnknownOutcome`; exactly one POST occurs and the
application must not retry. A request-build failure becomes `Error::Request`
without a wire request. A later details failure leaves the earlier acceptance
available to the application. For an adversarial schema-valid 2xx acceptance
with a nonzero `ErrorCode`, I predict the numeric field is exposed unchanged;
the docs do not promise promotion into `Error::Api`.

### Workflow 2: page through outbound messages, then retrieve an item

Call `outbound.list(2, 0).send().await`, explicitly request another page with
`outbound.list(2, 2).send().await` if needed, and retrieve a returned item's ID
using `outbound.details(&item.message_id).send().await`.

Expected wire: bodyless `GET /messages/outbound?count=2&offset=0`, then offset 2,
with JSON Accept and the server-token header, followed by the same details GET
as workflow 1. Count/offset are query values, not path segments or JSON. The
generated calls should accept ordinary integer literals and a borrowed String;
no extension-trait imports or generated endpoint type annotations should be needed.

Expected result: each call yields exactly one `MessagePage { total_count,
messages }`. There is no automatic pagination or completion wrapper. In this
stable fixture, the application can stop when the accumulated count reaches
`total_count`, and an empty page also ends traversal. This is an application
decision, not a snapshot/delivery guarantee for changing provider data. On a
later-page failure the application retains earlier pages and reports incomplete
traversal. A malformed page fails as a whole, without returning its decoded
prefix. A failed detail lookup does not destroy the selected page/item.

Expected failure: 429 is `Error::Api` with integer Retry-After and no automatic
retry; invalid bounds are sent to the provider and reported as validation
errors. Malformed JSON or wrong media on a successful read is `Error::Request`,
not `UnknownOutcome`. Missing messages after retention can be provider errors.

### Secrets and scope

The token must reach the fixture but be absent from client/config/endpoint Debug
and formatted errors. Provider error message text is excluded from Error and
ErrorDocument Debug, but remains accessible through `document.message` and is
potentially sensitive. Successful message content is application data, not a
promise of blanket redaction. Use `.send()` throughout; `prepare()` would bypass
the SDK policy and is unnecessary for these tasks. No inbound, webhook, batch,
attachment or delivery-guarantee behavior is in scope.

## Public sources read before first call

- `examples/postmark-sdk/README.md` (first SDK content read).
- Generated rustdoc under `target/doc/concord_sdk_postmark/`: crate index,
  `Postmark`, `PostmarkConfig`, `PostmarkOutbound`, all three endpoint structs,
  `Email`, `Acceptance`, `Message`, `MessagePage`, `Error`, `ErrorDocument`,
  `PostmarkPolicy`, `http_client_builder`.
- Generated `concord_example_testserver` public rustdoc for fixture construction,
  requests, replies, bodies, gates and shutdown. No harness source was needed.
- [Postmark API overview](https://www.postmarkapp.com/developer/api/overview),
  read 2026-11-07: authentication, JSON, HTTP/provider errors and throttling.
- [Postmark Email API](https://postmarkapp.com/developer/api/email-api),
  read 2026-11-07: single-email route, JSON keys and acceptance fields.
- [Postmark Messages API](https://postmarkapp.com/developer/api/messages-api),
  read 2026-11-07: outbound search/details, count/offset bounds and message events.

Administrative reads: ancestor `AGENTS.md`, handoff `START_HERE.md`,
`DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`, `contracts/PHASES.md`,
`contracts/RESOURCES.md`, root/package manifests, git status and HEAD. An initial
overbroad filename-only inventory was truncated; it did not read file contents.
No SDK source, existing SDK tests, `FRICTION.md`, `PROVENANCE.md`, or rustdoc
source links have been read.

## Setup evidence

The package and lock entry already exist at the frozen baseline. Initial
`env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo doc -p concord-sdk-postmark
--no-deps --offline` generated public docs without a lockfile change. A subsequent
locked doc build also included `-p concord-example-testserver` and passed.
All subsequent Cargo invocations use `--locked` and four jobs, one at a time,
with the default worktree `target/`. No dependencies were added.

Resource sample on 2026-11-07 (after initial rustdoc generation): `df -h . /tmp`,
`du -sh target`, `free -h`, `nproc`: workspace filesystem 915 GiB / 205 GiB free;
target 1.2 GiB; memory 30 GiB / 24 GiB available; 20 CPUs. Existing artifacts
are reused; this is not a clean-build claim. No cache cleanup was required.

## Friction incidents (chronological)

1. Generated endpoint `send()` rustdoc displays
   `<PostmarkPolicy as SendWith<T>>::Output` rather than `Result<T, Error>`.
   Reading the public `PostmarkPolicy` trait implementation resolved it. No
   source lookup or trait import was needed. Suggested macro/docs improvement:
   link directly to the policy output implementation and show the concrete
   result where available; shared crates remain frozen.
2. The same public output type establishes that ordinary success drops HTTP
   metadata. A fixture trace header cannot be obtained from `Acceptance` or
   `MessagePage`; JSON metadata remains available. This was known before coding,
   not a failed test. SDK improvement to consider: an ordinary policy-preserving
   method returning value plus response metadata. Do not advise `prepare()` as
   a substitute, because it bypasses the send-outcome policy.
3. After first success, deeper timeout/metadata assertions required following
   the wrapped Concord `Error` to its public documentation. Generated
   `concord-core` rustdoc with a separate locked command, then read
   `target/doc/concord_core/struct.Error.html` and `enum.ErrorKind.html`.
   `transport().is_timeout()` confirms timeout and `head()` preserves response
   status/headers after a body failure; `status()` alone is only for status
   errors. Added the purposeful `concord::ErrorKind` import for these assertions.
   A short linked diagnostic example in the SDK docs would reduce discovery work.
4. A one-page API leaves traversal completion and retention of earlier pages to
   the caller. Added small application-owned `Listing` / `ListingStop` types,
   distinguishing total reached, empty page and failure while retaining messages.
   These types are needed for this application's report, not to call an endpoint.
   An empty page with an unchanged total of three stops traversal after two
   retained messages; it does not prove a stable complete provider snapshot.
   Ownership: application, with a useful SDK README example as a possible aid.

No compiler failures, misleading compiler diagnostics, surprising generated
names, unneeded imports/types or implementation-source lookups occurred. All
shared/core/macro improvements above are proposals only; no frozen files changed.

## First working call and observed comparisons

The first integration-test compile and run passed: two tests, zero failures,
0.02 seconds test execution. The send/inspect test's first call was
`api.outbound().send_email(receipt()).send().await`, followed by details using
the returned UUID. The list test also passed its two-page traversal and item
lookup. Tests ran concurrently, so no ordering claim is made about which test's
first HTTP request arrived first.

It took the README pattern, public response/error rustdoc and the fixture's
public rustdoc. No import of a Concord trait, generated type annotation, source
lookup, SDK change, dependency change or compiler correction was necessary.
`no_proxy()` was set on the HTTP builder for deterministic loopback routing.
Optional `HtmlBody: None` was omitted as predicted. Exact JSON, query pairs,
token/Accept/Content-Type headers, returned fields and request counts passed.

Failure and partial-result probes were added after this first success; the
initial predictions above remain unchanged. They also compiled and passed on
their first run. The final tests are exactly two caller tasks, with table-driven
failure fixtures inside each test.

| Prediction | Observed behavior / assertion |
|---|---|
| Send JSON, server-token header, Accept and optional omission | Exact method/path/JSON, single token header, no bearer auth and absent HtmlBody all passed. |
| Inspect the returned message ID | Details path uses the acceptance UUID; recipient, timestamp, ID, code and message round-trip. |
| Acceptance differs from delivery | Details remain Queued with empty MessageEvents; JSON Metadata is preserved. No delivery claim is made. |
| One list call is one page | Request count is one immediately after the first call; explicit count 2 / offsets 0 and 2 produce three items and one detail lookup. |
| Pagination completion belongs to caller | Total reached and empty-page stops are distinguished, including a fully empty search with no invented detail call. No further list request occurs. |
| Later failures preserve prior results | 429 or malformed second page retains the first two items; a retained ID can still be inspected. Failed details preserve the prior acceptance or completed three-item list. |
| Malformed page has no usable partial page | Even a second-page JSON prefix containing one complete object returns Request(Decode); the third item is not added. |
| Non-2xx preserves provider error information | 401, 422, 429 and 503 sends return Api with code and status; integer Retry-After survives. Invalid count/offset reaches the fixture unchanged and returns code 700. |
| Malformed/non-JSON error document preserves status | 502 remains Api with no document. HTTP-date Retry-After has no seconds value, consistent with the documented integer-only projection. |
| Timeout after send is unknown, never replayed | A fully recorded email receives a 200 head and stalled body. The result is UnknownOutcome(Transport), underlying is_timeout() is true, endpoint identity and response trace header survive. Exactly one POST is recorded through an additional 200 ms observation window. |
| Other unusable send responses are unknown | Broken JSON, wrong media, disconnect and a complete JSON payload with unfinished chunked transport each yield UnknownOutcome and exactly one POST. |
| Build/read failures are Request | Invalid token header is Request(Build) with zero requests; wrong-media read is Request(Metadata); broken page is Request(Decode) with its response head. |
| Explicit secret access differs from formatting | Fake token reaches the wire but is absent from config/client/scope/endpoint Debug and formatted errors; ErrorDocument Debug hides provider text, while explicit message access retains it. |
| Schema-valid 2xx with nonzero provider code is exposed | Synthetic ErrorCode 400 returns Ok(Acceptance) carrying 400. This matches the prediction, but callers must not treat this adversarial response as successful acceptance based solely on Result::Ok. |

**Prediction mismatches: none among the exercised expectations.** The 2xx
nonzero-code fixture is a robustness probe, not a claim that Postmark sends this
response for a single-email rejection. SDK-owned follow-up to consider: document
or validate the `Acceptance.error_code == 0` semantic invariant. Generic core
policy should not infer Postmark-specific codes.

The fixtures are authored here using the official wire contract and invented
addresses/UUIDs. `X-Fixture-Trace`, error messages and the numeric 429 fixture
code are synthetic test data; only their preservation is asserted. The consumer
tests do not independently exercise the 16 MiB / 64 KiB size thresholds, sender
verification, real retention expiry or actual provider delivery. Existing
package tests were executed without reading their implementation.

## Final verification

All Cargo commands used `env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4`; the first
doc command omitted `--locked` as recorded above, and all later ones included it.

- `cargo doc -p concord-sdk-postmark -p concord-example-testserver --no-deps
  --offline --locked`: passed.
- `cargo doc -p concord-core --no-deps --offline --locked`: passed, public error
  documentation only.
- `cargo test -p concord-sdk-postmark --test first_use --offline --locked`:
  initial happy workflows **2 passed** (0.02 s); expanded failure workflows
  **2 passed** (0.47 s).
- `cargo test -p concord-sdk-postmark --offline --locked`: final package check
  **10 integration tests passed** (2 new + 8 existing), **1 compile doctest
  passed**, zero failures. Repeated after adding the failed-list-item inspection
  case; final consumer tests took 0.47 s.
- `rustfmt --edition 2024 examples/postmark-sdk/tests/first_use.rs`: completed;
  only the new caller file was formatted.
- Git comparison against the pinned baseline confirms no edits to SDK `src/`,
  root manifest/lockfile, shared crates, checks, scripts or other examples.
  Only this record and `tests/first_use.rs` are added. No commit was attempted.

Final resource sample: target about 1.4 GiB; workspace filesystem about 204 GiB
free. No new crate, version change, dependency justification or PROVENANCE edit
was needed. No real Postmark API request was made: live provider calls **NOT_RUN**.

## Assistance label

**docs-only**. SDK and Concord public docs, fixture public rustdoc and official
provider docs were sufficient. SDK implementation, existing test source,
`FRICTION.md`, `PROVENANCE.md` and generated rustdoc source views were never read.
The new consumer test file was read back and edited as part of this task.
