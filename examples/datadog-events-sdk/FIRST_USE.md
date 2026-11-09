# Fresh consumer: SDK42 Datadog Events

Current API note (P4.2): this evidence records the earlier API. Paging `_from` twins now use the base factory followed by `.starting_at(Resume::At(key))`; Datadog/Notion traversal cycles use `PagesError::Repeated`. See [migration](../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All application requests will target
`concord-example-testserver` on `127.0.0.1`; all keys are synthetic.

## Predictions

Written before application code or a successful SDK call, from the README and generated
public rustdoc only (plus the official pages below).

### Workflow 1: list a bounded range, inspect one returned event

- Construct `Credentials::new(api_key, application_key)`, `DatadogEvents::http_client()`,
  and `DatadogEvents::new(http, Site::Eu, credentials)`. Override the base URL with the
  loopback server's URL before sending. The production URL, inspected without sending,
  should be `https://api.datadoghq.eu/api/v2/events`.
- Call `list(TimeRange::from_millis(1767225600000, 1767229200000)?)`
  `.set_query("service:checkout env:prod").set_sort(Sort::Descending)`
  `.set_limit(PageLimit::new(2)?).send().await`.
- Wire: `GET /api/v2/events`; decoded query keys `filter[from]=1767225600000`,
  `filter[to]=1767229200000`, `filter[query]=service:checkout env:prod`,
  `sort=-timestamp`, `page[limit]=2`. No cursor on the first request; no body.
  Headers include exactly one each of `DD-API-KEY`, `DD-APPLICATION-KEY`, and
  `Accept: application/json`. No bearer authorization. Host is the loopback authority
  during tests, otherwise the selected site's API authority.
- Return `EventsPage` with ordered `data`, optional `links` and `meta`. Inspect the first
  returned ID with `page.event(id)` locally, without another HTTP request. Assert its ID,
  type, message, timestamp, tags and one raw attribute without modeling event schemas.
  Warnings indicate potentially partial data but do not fail the page. Request ID,
  elapsed time and status remain accessible. `next_cursor()` carries `meta.page.after`;
  manual continuation preserves filters and adds `page[cursor]`. Absent `after` ends
  pagination; `links.next` is informational and never followed automatically.
- Empty application key fails before sending with `AuthError::MissingApplicationKey`,
  convertible to `Error::Authentication`, no response status/head. Wrong-region keys
  receive a fixture 403 and become `Error::Authentication(AuthError::Rejected)`;
  401 has the same class. The rejected host is loopback and its inferred site is `None`,
  even though the configured client site stays `Eu`. This is an authentication rejection,
  not proof that the SDK can distinguish wrong region from wrong keys.
- Status determines non-2xx classification even with an unusable body: 400 `BadRequest`,
  429 `RateLimited` with headers and no retry, other statuses `Provider`. Provider
  `{"errors":[...]}` messages are available only by explicit access. A malformed JSON 2xx
  body produces `Error::Concord` / `Decode`; a non-JSON 2xx produces `Metadata`, keeping
  the response head. The documented size/transport limits are not predictions of live
  provider behavior and need not all be exercised by this small caller.
- Credentials/client/prepared request/error `Debug` and error `Display` must not contain
  either synthetic secret, even if provider error text echoes them. Native request key
  headers must be marked sensitive. Explicitly reading headers/messages is the documented
  boundary; assertions may compare the synthetic values without logging them.

### Workflow 2: search a filter and follow pagination

- Build `SearchRequest::new().query("source:datadog-agent service:checkout")`
  `.from("2026-01-01T00:00:00Z").to("2026-01-01T01:00:00Z")`
  `.timezone("GMT").sort(Sort::Ascending).limit(PageLimit::new(2)?)`.
  Use `search_pages(request).max_pages(nonzero_cap)`, then `next_page().await`.
- Wire: `POST /api/v2/events/search`, no query string, both key headers, JSON accept and
  content type. Initial JSON body:

  ```json
  {"filter":{"query":"source:datadog-agent service:checkout","from":"2026-01-01T00:00:00Z","to":"2026-01-01T01:00:00Z"},"options":{"timezone":"GMT"},"sort":"timestamp","page":{"limit":2}}
  ```

- Traversal is lazy. Each next call sends the same body plus `page.cursor` equal to the
  previous response's `meta.page.after`, preserving opaque punctuation. It follows the
  cursor even after an empty page; it never requests `links.next`. Each yielded page has
  `items` and `PageInfo` (`request_id`, `status`, warnings); the adapter does not promise
  `elapsed` or raw links. Terminal page lacks `after`; subsequent calls return `Ok(None)`
  without sending. Warnings and complete cursor traversal can coexist: traversal
  completion does not guarantee complete provider data.
- A cap with a cursor remaining produces `pagination::PagesError::Bounded`, retaining
  a resume/checkpoint. `search_pages_from` with the same request resumes at that cursor
  without replaying earlier pages. This explicit bound/resume API is needed to test
  honest completion rather than silently truncating results.
- A failed later page becomes `PagesError::Provider` containing the SDK's typed error
  and the failed cursor. `items().collect_complete()` returns `Incomplete` with events
  already collected and its error; flattening intentionally loses warnings/metadata.
  It must not return partial data as a successful complete collection or retry a 429.
- Empty or repeated continuation cursors produce a protocol error instead of a restart
  or loop. Earlier pages remain caller-owned. Malformed success JSON on a later page
  remains a Concord decode error with its response head. Authentication, status and
  redaction semantics are the same as for list.

## Public documentation read

Official pages read on **2026-11-07**, only for research (no API requests):

- [Authentication](https://docs.datadoghq.com/api/latest/authentication/): read access
  requires API and application keys; site configuration matters.
- [Get a list of events](https://docs.datadoghq.com/api/latest/events/get-a-list-of-events/):
  v2 GET path, range/query/sort/page parameters, response envelope and documented statuses.
- [Search events](https://docs.datadoghq.com/api/latest/events/search-events/): v2 POST,
  filter/options/page body, cursor continuation, warnings and documented statuses.

The endpoint examples now use bearer tokens; the authentication page still describes
API/application keys. This SDK explicitly documents its narrower two-key contract.
Fixtures use the documented common envelope, representative values and arbitrary opaque
cursors, not captured production responses. Event-specific attributes remain raw JSON.

Local public reads before the first working call: SDK `README.md`; generated SDK rustdoc
for the client, credentials/errors, requests, event/page models and pagination alias;
generated testserver rustdoc; generated Concord and pagination rustdoc as needed. Workspace
and package manifests and git status were inspected for build constraints. No SDK source,
existing SDK tests, `FRICTION.md` or `PROVENANCE.md` was read.

## Build constraints and initial observations

- Working tree started clean. HEAD is `38d62dce548e920c8430a4b99cc99c6487aea80b`;
  the task's frozen shared-crate baseline is `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`.
  No shared files will be edited or reset.
- Workspace membership and package/dev dependencies already exist. Initial build:
  `env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo doc --offline -p concord-sdk-datadog-events --no-deps`
  succeeded without `--locked`. All later cargo commands use `--locked`, offline mode,
  four build jobs and the worktree's default `target/`. No new dependencies are planned.
- The web reader rejected the three official pages as unsupported `text/markdown`;
  reading those exact URLs with `curl` succeeded. This was research-tool friction,
  not an SDK error.

## First working call and observations

The first SDK request succeeded on the first compile/run attempt. Command:
`cargo test --locked --offline -p concord-sdk-datadog-events --test first_use caller_lists_bounded_events_and_inspects_one -- --exact`
(with `CARGO_BUILD_JOBS=4` and `CARGO_TARGET_DIR` unset). Result: **1 passed**, no
compiler diagnostics, 0.09 seconds test runtime.

It took the README's constructor and list chain, rustdoc's `with_base_url(server.url())`,
and the testserver's public `Reply::json`/recorded-request helpers. The first working
call was the bounded GET returning a synthetic checkout deployment. Local inspection
did not issue a second request. The same first run also passed manual continuation,
local missing-application-key rejection, fixture 401/403 rejection with reflected
synthetic keys, malformed 400/429/503 documents, malformed 200 JSON and wrong media type.
No SDK source or existing tests were needed or read.

The first search test also compiled without diagnostics. Two runtime runs failed on
the repeated-cursor timing assumption described below; the subsequent combined run
passed both caller tasks. The final application uses the normal `.list(...).send()`
and `.search_pages(...).next_page()` paths. Preparation is used only to inspect site
hosts and sensitive header flags without sending. Bound/resume and incomplete
collection APIs are used specifically to check honest completion and recovery.

After the first working call, public rustdoc for pagination `Progress` and SDK
`ProtocolError` was read to assert counters and concrete protocol variants. Only this
new test file was formatted. The final package test command executed existing tests
and doctests but their source files were never opened.

## Friction incidents (chronological)

1. The official-doc web reader returned unsupported-content errors on all three exact
   URLs. `curl` to the same pages succeeded. Research-tool friction only.
2. Initial rustdoc HTML extraction included large blanket-implementation/script sections
   and truncated the tool output. Extracted main documentation and stopped before trait
   boilerplate to read the relevant public contracts. Tooling friction only; no source.
3. `cargo doc --no-deps` exposes `SearchPages = Pages<Event, Cursor, PageInfo, Error>`,
   but does not generate dependency pages for bound/resume and `Incomplete` APIs.
   Generated public rustdoc separately for `concord-pagination` and `concord` to learn
   those contracts without source lookups. Ordinary list and search examples were clear.
4. First combined caller run compiled cleanly, but search's bad-continuation check failed:
   the second `next_page()` returned `Ok(Some(Page { items: 1, next: Some(..), .. }))`
   where the caller expected a protocol error. The preceding search completion,
   bound/resume, partial collection and explicit recovery assertions had passed. The
   caller's original assertion did not identify the table-driven case. A second run
   with the case name identified `repeated`. A third run confirmed that this page is
   returned, then the following call gives `ProtocolError::RepeatedCursor` **before any
   third HTTP request**. Updated the consumer assertion to encode this timing, preserving
   the prior page and the repeated-cursor page as caller-owned results. The error class
   and no-loop guarantee match the docs; same-call rejection was the consumer's stronger
   interpretation of their wording. No SDK source lookup or implementation fix.

No extra imports or generated request types had to be guessed; both tests compiled on
their first compilation. `NonZeroUsize` and `pagination::PagesError` are needed for the
explicit cap/resume assertions. There was no further naming, diagnostic or completion
friction. Source lookups: **none**.

Documentation changes I would propose: clarify in SDK pagination docs that repeated
cursors fail when requested, after the advertising page has been yielded; mention the
extra dependency rustdoc command for consumers using `--no-deps`. No shared-crate change
was required or made, and the fixed client → scope → endpoint hierarchy was preserved.

## Prediction comparison

| Prediction | Observed result |
|---|---|
| EU site host, bounded GET, two key headers and JSON accept | Matched. Prepared EU host without sending; loopback saw exactly the predicted decoded query values, headers and empty body. |
| Inspect a returned event locally | Matched. ID, type, common attributes and raw title were accessible; request count stayed one. Missing ID returned `None`. |
| List metadata/warnings and manual cursor continuation | Matched. Warning did not fail the response; request ID, elapsed, status and informational link were retained. Second GET preserved inputs and round-tripped the opaque cursor; no cursor on the terminal page. |
| Missing app key fails locally; wrong-region keys are authentication errors without secrets | Matched in fixtures. Missing key had no head/status and sent nothing. Synthetic EU keys paired with a US1 client produced fixture 401/403 rejections. US1 production host was inspected without sending, then overridden to loopback. Rejection named `127.0.0.1`, inferred site `None`; configured client site stayed US1. Both synthetic keys were absent from Debug/Display and nested error sources, despite deliberate provider body/header echoes. This does not establish live regional behavior. |
| Status remains authoritative despite malformed provider bodies | Matched for 400/429/503. Kept response status, operation and retry header; document decoding failed separately. Exactly one request each, no retry. |
| Malformed/non-JSON success bodies keep head with Decode/Metadata | Matched for both 200 fixtures. |
| Lazy search POST, stable JSON filter/options/sort/limit, cursor in JSON | Matched for every recorded search request, including punctuation in cursors. No query string or bearer header. |
| Empty page with continuation is not the end; absent continuation is | Matched across three pages. Exact cap on the terminal page was successful exhaustion. Further calls returned `Ok(None)` without HTTP. Informational links were never followed. |
| Search page metadata survives; warnings do not prevent traversal completion | Matched. Kept per-page request IDs, status and warnings. The adapter exposes `PageInfo`, not list's entire metadata envelope. Complete traversal still contained a warning about potentially partial provider data. |
| Smaller cap is explicitly incomplete and resumable | Matched. `Bounded`, retained checkpoint, then `Ended`; no request beyond cap. Resume fetched the remaining empty and terminal pages without replaying the prefix. |
| Failure after a prefix preserves events, typed error, failed cursor and counters | Matched. `Incomplete` kept the first event and 429 with rate header; admitted=2, succeeded=1, failed=1. No automatic retry. Explicit resume returned only the remaining event. Flattening was used only where metadata loss was intentional. |
| Empty/repeated cursor and malformed later page stop the traversal | Empty cursor and malformed JSON failed on that response. Repeated cursor's error class/no-loop matched, but its timing differed from the first application's assumption: the advertising page is yielded; the next call fails before another wire request. After each failure, calls returned `Ended`, preserving caller-owned prior results. |

No live provider behavior was predicted as experimentally established. Body-size limits,
transport interruptions, TLS/HTTP2 and real key permissions were not independently
exercised by these two consumer tasks.

## Final validation

- `cargo test --locked --offline -p concord-sdk-datadog-events --test first_use`:
  **2 passed**, no failed/ignored tests (after the documented timing correction).
- After formatting and making the wrong-region fixture's US1 configuration explicit,
  `cargo test --locked --offline -p concord-sdk-datadog-events`:
  **29 integration tests passed** (2 first-use, 19 protocol, 8 workflows), plus
  **6 doctests passed**. No compiler warnings; no unit tests were defined.
- `rustfmt --edition 2024 examples/datadog-events-sdk/tests/first_use.rs` completed.
- Every cargo command used four build jobs and default worktree `target/`; the initial
  offline doc build omitted `--locked`, all subsequent cargo commands included it.
- `Cargo.lock` is byte-for-byte identical to its initial snapshot. No dependency added,
  no package version changed; no provenance entry was needed. Workspace membership
  was already present, so root `Cargo.toml` was untouched.
- Final changes are only `FIRST_USE.md` and `tests/first_use.rs` in this package.
  SDK `src/`, existing tests, `FRICTION.md`, `PROVENANCE.md`, all shared crates, checks,
  other examples and scripts were not edited. No commit attempted.
- Live provider calls: **NOT_RUN**. All sent SDK requests used local fixtures.

## Assistance label

**docs-only**. SDK source, existing test source, `FRICTION.md` and `PROVENANCE.md`
were never read. No campaign files or other worktrees were consulted. Public rustdoc
source links were not followed.
