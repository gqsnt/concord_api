# First use of `concord-sdk-sentry` (fresh consumer)

Role: fresh consumer, workflows run only against `examples/testserver` (live provider calls:
**NOT_RUN**). Written before reading any of `src/`, `tests/`, `FRICTION.md`, `PROVENANCE.md`.

## Material read before predicting

- `examples/sentry-sdk/README.md`, `examples/sentry-sdk/Cargo.toml` (dependencies only).
- Rustdoc: `cargo doc -p concord-sdk-sentry --no-deps` (all 37 pages: `Sentry`, the four endpoint
  structs, scopes, `Issue`, `Event`, `Response`, `RateLimits`, `Problem`, `ProviderError`, `Error`,
  `HttpClass`, `SendPolicy`, `Settings`, `SentryConfig`, `Listing`, `Resume`, `PagesError`,
  `http_client`, constants).
- Because `--no-deps` leaves `Pages`, `Page`, `Progress`, `ResponseMeta`, `ReceiveReport` and
  `concord::Error` unlinked, I also generated Concord's public rustdoc
  (`cargo doc -p concord -p concord-core -p concord-pagination -p concord-retry -p concord-ratelimit
  -p concord-example-testserver --no-deps`, then `concord-retry/concord` and
  `concord-pagination/link` features) and read `Pages`, `Page`, `Progress`, `Outcome`,
  `ResponseMeta`, `ReceiveReport`, `Report`, `StopReason`, `concord::Error`, `ErrorKind`, and the
  testserver's `TestServer`, `Recorded`, `Reply`, `Body`, `Gate`, `Script`.
- Official Sentry documentation, read 2026-11-07 (read-only):
  - https://docs.sentry.io/api/pagination/ — `Link` header with `rel="previous"|"next"`,
    `results="true"|"false"`, `cursor`; keep following next while `results="true"`; cursors are
    returned in both directions even on empty pages; cursor form `id:offset:is_prev`.
  - https://docs.sentry.io/api/ratelimits/ — per caller+endpoint fixed-window frequency limit and
    a concurrent-request limit; headers `X-Sentry-Rate-Limit-Limit`, `-Remaining`, `-Reset`
    (UTC epoch seconds), `-ConcurrentLimit`, `-ConcurrentRemaining`; limits are tied to caller
    identity, not the token. The page does not name the rejection status or `Retry-After`.
  - https://docs.sentry.io/api/events/ — section index only (List an Organization's Issues,
    Retrieve an Issue, List an Issue's Events, Retrieve an Issue Event). I did not open the
    per-endpoint sub-pages (they are not in my allowed list), so paths and fields below come from
    the SDK rustdoc.

## Predictions

Common setup (both workflows):

```rust
let policy = SendPolicy::default();             // or SendPolicy::new(Settings { .. })?
let api = Sentry::with_token(http_client()?, TOKEN, policy)?.with_base_url(server.url())?;
```

- `with_token` builds `authorization: Bearer <token>` as a sensitive header. An invalid token
  (empty, whitespace/control characters) returns `Error::Credential` before anything is sent.
- `with_base_url` returns a new `Sentry` (fallible). **Uncertain prediction:** the declared base
  URL is `https://sentry.io/api/0` and endpoint paths start at `/organizations/...`, so with
  `with_base_url(server.url())` the loopback server sees `/organizations/acme/issues/` (without
  `/api/0`). Self-hosted/staging callers would then pass `https://host/api/0`.
- Every request: header `authorization: Bearer sntryu_…` and (predicted) `accept:
  application/json` from the JSON format; no body; no redirects followed (`http_client`).

### Workflow 1 — list organization issues by cursor, retrieve a chosen issue

Calls:

1. `api.organization("acme").issues().set_query("is:unresolved").set_limit(2)` then either
   `.send().await` (one page) or `.pages(Resume::Start, cap)` and `next_page().await` in a loop.
2. `organization.issue(&summary.id).get().send().await`.

Wire:

1. `GET /organizations/acme/issues/?limit=2&query=is%3Aunresolved` (declared order `limit`,
   `cursor`, `query`; `limit=100` is sent when not set; `cursor` absent on the first page;
   `query` absent unless set, `set_query("")` sends `query=`). The colon may or may not be
   percent-encoded; the decoded pair is `query=is:unresolved`.
2. Next page: same path and filters plus `cursor=<value from the next link>`, i.e.
   `?limit=2&cursor=…&query=…` — the adapter rebuilds the request from the generated operation, it
   never dispatches the Link URL.
3. `GET /organizations/acme/issues/<id>/`.

Returns:

- `send()` → `Response<Vec<Issue>>`: `value` (issues with `id`, `short_id` from `shortId`,
  `title`, `status`; extra fields ignored), `meta` (status 200, headers, operation
  `Sentry.organization.issues`), `next_cursor` = `Some(cursor)` only when the unique
  `rel="next"` link has `results="true"`, `rates` parsed from `X-Sentry-Rate-Limit-*` (missing
  header → `None`), `retry.retry.attempts == 1` on a clean call.
- `pages(...).next_page()` → `Ok(Some(Page { items: Vec<Issue>, next: Option<String>, meta:
  ResponseMeta }))` per page; `Ok(None)` after the page whose next link says `results="false"`
  (or has no next link), and on every later call; with a cap reached while a next page exists →
  `Err(PagesError::Bounded { resume: Resume::At(cursor), .. })`, then `Ended` on later calls.
- `get().send()` → `Response<Issue>` with `next_cursor == None`.

Failures:

- Non-2xx → `Error::Provider(ProviderError { class, meta, document, retry, .. })`. 401 →
  `HttpClass::Unauthorized`, `document == Ok(Problem { detail })` for `{"detail": "..."}`, one
  attempt (no retry). 403/404 likewise single attempt. 400 → `BadRequest`, no retry.
- 429 and 502/503/504 are retried (max 3 attempts, backoff 100ms→2s, Retry-After as minimum); if
  every attempt fails the last provider error comes back with `retry.retry.attempts == 3`. 500 is
  `Server` but not retried.
- A non-JSON error document (HTML 502 from a proxy) still yields `Error::Provider` with the
  status, and `document == Err(..)`.
- 2xx with malformed/truncated/empty JSON or a wrong-shape body → `Error::Concord(e)` with
  `e.kind() == ErrorKind::Decode`; wrong `content-type` → `ErrorKind::Metadata`; >1 MiB →
  `ErrorKind::Limit`.
- Invalid `X-Sentry-Rate-Limit-*` values on a 2xx → `Error::RateMetadata { reason, meta }`.
- Pagination: a next link without a `results` flag, with a malformed Link, two next links, a
  link to a different origin/path, a different `query` filter, a `cursor="…"` parameter
  disagreeing with the URL's `cursor`, or a next cursor equal to the current cursor →
  `Error::Pagination { reason, meta }` (from `send()`), wrapped as `PagesError::Provider` in a
  traversal with `failed` = the page that was requested. I expect the Link URLs must use the
  same origin as the base URL, so loopback fixtures must emit `http://127.0.0.1:<port>` links.
- Partial results: pages already returned stay with the caller; a failure on page 2 yields
  `PagesError::Provider { error, failed: Resume::At(cursor_of_page_2), .. }` and a new
  `pages(failed, cap)` resumes exactly there.
- Secrets: `format!("{err}")`/`{err:?}`, `{response:?}`, `{issue:?}`, `{page:?}` never contain
  the token, the cursor value or the search query; `issue.title` is readable through the field.

### Workflow 2 — page an issue's events via Link, retrieve an event

Calls:

1. `let issue = api.organization("acme").issue("4567891230");`
   `issue.events().set_per_page(2).pages(Resume::Start, cap)` → `next_page()` loop.
2. `issue.event(&summary.event_id).send().await` (by `eventID`, not storage `id`).

Wire:

1. `GET /organizations/acme/issues/4567891230/events/?per_page=2` (no `full`, no `query`),
   then `?per_page=2&cursor=<next cursor>`.
2. `GET /organizations/acme/issues/4567891230/events/<eventID>/`.

Returns: pages of `Event { id, event_id (eventID), group_id (groupID), title, entries: [] }`;
the detail call returns `Event` with non-empty `entries`; `event.value.group_id` equals the issue
id.

Failures (in addition to those of workflow 1):

- **Next link with `results="false"` stops**: even though it carries a cursor, `next_cursor` is
  `None`, the traversal returns `Ok(None)`, and no further request reaches the server.
- An empty page with `results="true"` continues.
- **Retry scheduling respects concurrency**: with the default one simultaneous request per
  endpoint, a retried attempt (after a 429) waits for a concurrency lease again; while another
  task holds the event endpoint's lease (its response head is delayed), the retry is not sent.
  The server therefore never sees two overlapping event-retrieve requests.
- `X-Sentry-Rate-Limit-ConcurrentLimit: 1` on a response caps local concurrency to 1 even if
  `Settings.concurrency` is larger; `X-Sentry-Rate-Limit-Remaining: 0` with a future `Reset`
  delays the next request on that endpoint until the reset (frequency is separate from
  concurrency).
- Retries count separately from pages: one page served after a 503 has `progress().succeeded`
  = 1 page but `Response.retry.retry.attempts == 2` (not observable through `Page`, whose meta is
  only `ResponseMeta`; I expect to need `send()` for retry accounting).
- Unknown event id → 404 `Error::Provider { class: NotFound }`, single attempt.

## First working call (what it took)

`tests/first_use.rs`, first version: `Sentry::with_token(http_client()?, TOKEN,
SendPolicy::default())?.with_base_url(server.url())?`, then
`api.organization("acme").issues().send().await`, against a `TestServer` replying one issue as
JSON. Imports: `concord_sdk_sentry::{http_client, SendPolicy, Sentry}` and
`concord_testserver::{Recorded, Reply, TestServer}`; no direct Concord dependency, no
`Cargo.toml` change. It compiled and passed on the first `cargo test` run (2026-11-07). The
server saw `GET /api/0/organizations/acme/issues/?limit=100` with headers `authorization:
Bearer …`, `accept: application/json`, `host` (in that order; no user-agent). The `/api/0`
prefix in the path contradicted my prediction (incident 2).

## Application built

`tests/first_use.rs`, 9 tests, all against `examples/testserver`. Synthetic values: org `acme`,
issue `4567891230` / `BACKEND-7Q`, `sntryu_…` token, Sentry-style cursors
(`1730937600000:2:0`, `0:2:0`), Link headers with both `previous`/`next`, `results` and
`cursor` parameters, `X-Sentry-Rate-Limit-*` headers.

Workflow 1 (issues):

- `workflow1_issues_by_cursor_then_retrieve_chosen_issue`: `issues().set_query(..).set_limit(2)
  .pages(Resume::Start, 10)` over three pages. The last page's next link carries a cursor with
  `results="false"`. Then `issue(&chosen.id).get().send()`. Assertions cover wire targets and
  headers, sticky `Ok(None)`, exactly 3 list requests, the decoded issue, the `RateLimits`
  fields, `retry.attempts == 1`, and Debug output withholding token, cursor, filter and title.
- `workflow1_single_page_cursor_and_bounded_resume`: `send()` with `next_cursor` and
  `maybe_cursor`; `query=""` versus omitted; cap 1 → `Bounded { resume: At(cursor) }` → `Ended`
  → a new `pages(resume, 5)` continues from that cursor with the template's `limit=100`.
- `workflow1_failures_are_typed_and_bounded`:
  - `Credential` for an empty token, a CRLF-injected token and `two words`.
  - 400/401/403/404/500 each take a single attempt; the 401 carries a typed `Problem`.
  - A 429 with `Retry-After: 1` is retried, with a wait of at least 1s.
  - A 502 HTML page is retried 3 times and its unreadable document is `Err`.
  - A `Retry-After` beyond the deadline gives `RetryStopped`.
  - Bodies: truncated, wrong-shape and empty JSON give `Decode`; HTML gives `Metadata`; more than
    1 MiB gives `Limit`.
  - Bad quota metadata gives `RateMetadata`.
- `workflow1_invalid_links_are_pagination_errors`: missing or invalid `results` flag, another
  origin, a changed `query` filter, conflicting cursors, two next links, and an immediate cursor
  loop inside a traversal.

Workflow 2 (events):

- `workflow2_events_follow_link_until_results_false_then_retrieve_event`: `events()
  .set_per_page(2).pages(..)` using the documentation's `?&cursor=` link form. Page 2 first
  answers 503 (retried inside the page), then comes back empty with `results="true"`. Page 3's
  next link has `results="false"` with cursor `0:6:0`, which is never requested. Page sizes are
  `[2, 0, 1]` and `progress().succeeded == 3`. Then `issue.event(&chosen.event_id).send()` (path
  uses `eventID`, not storage `id`), `entries`, `group_id`, and an unknown event gives 404 in one
  attempt.
- `workflow2_failed_page_keeps_prefix_and_resumes`: a 500 on page 2 gives
  `into_provider_error()` → (`Error::Provider{Server}`, `failed = At("0:1:0")`,
  `progress.succeeded == 1`). The prefix stays with the caller, and a new traversal from `failed`
  completes.
- `workflow2_retry_waits_for_concurrency_lease` (**retry scheduling must not ignore
  concurrency**): two concurrent event reads. The first attempt gets 429 `Retry-After: 0`; the
  other read's head is held for 400ms. With the default single slot per endpoint, the retry
  arrives only after that hold, and no request overlaps at the server. Control: with 2 local
  slots the retry overlaps the held request.
- `workflow2_reported_concurrent_limit_caps_local_concurrency`: `Settings.concurrency = 4`, but a
  response reporting `ConcurrentLimit: 1` serializes the next two reads. Control: without the
  header they overlap.
- `workflow2_frequency_remaining_zero_waits_for_reset`: `Remaining: 0` with an absolute
  `Reset` 2s ahead; the next request reaches the server after the reset second.

Results (2026-11-07):

- `cargo test -p concord-sdk-sentry --test first_use --locked --offline`: 9 passed, 3 repeated
  runs plus one `--test-threads=1` run all green.
- `cargo clippy -p concord-sdk-sentry --tests --locked --offline`: no warnings.
- Whole package `cargo test -p concord-sdk-sentry --locked --offline`: unit 0, `first_use` 9,
  `protocol` 14, `workflows` 15, doctest 1, all passing. The author's suites were run, not read.
- `Cargo.lock` and the root `Cargo.toml` are unchanged: the package was already a member and
  locked, and I added no dependencies.

Observed timings (`--nocapture`, one run):

- Lease test: arrivals at 0 / 101 / 503 ms. The other caller's first send waited ~100ms after
  the 429, and the retry waited for the 400ms hold.
- Control (2 slots): 0 / 0 / 101 ms.
- Reported-limit test: 0 / 101 / 503 ms, with a cooldown of about 100ms after each
  `ConcurrentRemaining: 0`.
- Frequency test: the second request arrived 1.5ms after the reset second (gap 1.63s).
- 502 waits: `[100ms, 200ms]`, stop `Attempts`.

## Friction incidents (in order)

1. **Undocumented dependency types (docs).** `cargo doc --no-deps` renders `Listing<T> =
   Pages<T, String, ResponseMeta, Error>`, `Response.meta: ResponseMeta`, `retry:
   ReceiveReport`, `Error::Concord(Box<Error>)` and `Error::Admission(Error)` with no links. The
   two inner `Error`s look like the SDK's own `Error`. To learn `next_page()`, `Page { items,
   next, meta }`, `progress()` and the `ReceiveReport` fields, I had to generate Concord's rustdoc
   too, including the non-default `concord-retry/concord` feature.
2. **Base URL scope unclear (docs).** The rustdoc line "Request: GET
   https://sentry.io/api/0/organizations/{organization}/issues/ (the declared base URL; the
   client's with_base_url replaces it)" does not say where the base ends. I predicted `/api/0`
   belonged to the base. Observed: the base is the origin, and the wire path is
   `/api/0/organizations/…`. A self-hosted caller must pass `https://host`, not
   `https://host/api/0`; neither README nor rustdoc says so.
3. **Surprising generated name.** The path placeholders render as `{organization}` but
   `{issue.issue}` (the issue scope's argument is shown scope-qualified). Harmless, but it reads
   like a field access.
4. **My own test bug, not SDK friction.** `match t.0.lock().unwrap().len() { … t.record(..) }`
   kept the guard alive and deadlocked the responder thread; the first full run hung until I
   stopped it. Separately, the testserver's `Reply` is not `Clone`, so table-driven cases need
   `fn() -> Reply` factories.
5. **Per-page metadata is head-only.** Inside `pages()`, a `Page.meta` is `ResponseMeta`, so the
   per-page `RateLimits` and retry `ReceiveReport` that `Response` carries are not available. A
   traversal caller would have to re-parse `X-Sentry-Rate-Limit-*` from `meta.headers()`, and
   retry accounting is only visible through `progress()` and server counts. I predicted this.
6. **Deadline changes the error class.** A 429 whose `Retry-After` exceeds the remaining
   deadline returns (in ~8ms) `Error::RetryStopped { report: stop=Deadline, meta: Some(429 head)
   }`, not `Error::Provider { class: RateLimited }`, and the 429 problem document is dropped. A
   caller matching `Provider(p) if p.class == RateLimited` misses it and must check
   `meta.status()`. The README does say "A deadline returns Error::RetryStopped", but I did not
   predict it.
7. **Types not re-exported.** `StopReason` (in `RetryStopped.report.retry.stop`), `Progress` and
   `Outcome` are not re-exported by the SDK. Naming `StopReason::Deadline` would need a direct
   `concord-retry` dependency, so the test compares `format!("{:?}", stop) == "Deadline"`.
8. **Author advice in a caller's error.** An empty 200 body gives a `Display` of "…the body is
   empty or only whitespace, which is not JSON; omit `->` if this endpoint returns no body, or
   use a format that accepts an empty one". The advice is for the SDK author; a caller of a
   generated SDK cannot act on it.
9. **Coarse pagination reason.** A next link pointing at another origin reports "next Link leaves
   the current collection", the same wording a different path would get. The README distinguishes
   origin from collection; the diagnostic does not.
10. **Variant name in nested errors.** A pagination validation failure inside a traversal arrives
    as `PagesError::Provider { error: Error::Pagination { .. } }`. "Provider" there means "the
    page factory failed", not "Sentry returned an error". Readable once known, but a misleading
    first impression.

No source lookups were needed: everything above came from README, rustdoc and wire observation.

What I would change in shared crates (recorded only; nothing outside `examples/sentry-sdk`
edited):

- concord-macros generated docs: state the declared base URL and the path separately, and render
  `{issue}` rather than `{issue.issue}`, or explain the qualified form.
- concord-core/codecs: keep declaration advice ("omit `->`") out of the caller-facing `Display`,
  for example behind an author-facing accessor.
- SDK-level (author's call, `src/` not touched): re-export `StopReason`/`Progress`/`Outcome`,
  expose the status or class on `RetryStopped` (or a helper `Error::status()`), and consider
  `RateLimits` in page metadata.

## Predictions versus observed

| Prediction | Observed | Verdict |
|---|---|---|
| `with_token` validates; bad token → `Credential` before sending | `""`, CRLF, `two words` → `Credential` | match |
| Base URL is `https://sentry.io/api/0`, loopback path `/organizations/…` | Path is `/api/0/organizations/acme/issues/`; base is the origin | **mismatch** |
| `authorization: Bearer …`, `accept: application/json` | Exactly those, plus `host`; no body | match |
| Query order `limit, cursor, query`; `limit=100` default; `query` absent unless set; `set_query("")` → `query=` | `?limit=2&cursor=…&query=…`, `?limit=100`, `query=` | match |
| Colon may or may not be encoded | `query=is%3Aunresolved+assigned%3Ame`, `cursor=1730937600000%3A2%3A0` (space as `+`) | match (hedged) |
| Next request rebuilt from the operation with the same filters plus cursor | Same path/filters, cursor projected; links that omit `per_page` keep the template's value | match |
| `next_cursor` only for a unique `results="true"` next | As predicted | match |
| `results="false"` with a cursor stops, no further request | `Ok(None)`, sticky; cursor `…:6:0`/`0:6:0` never requested | match |
| Empty page with `results="true"` continues | Page sizes `[2, 0, 1]` | match |
| Cap with more data → `Bounded { At(cursor) }`, then `Ended` | As predicted; resume works | match |
| Missing/invalid flag, other origin, changed filter, conflicting cursor, two next links, cursor loop → `Pagination` | All `Error::Pagination` with 200 head; loop arrives as `PagesError::Provider { error: Pagination, failed: At(cursor) }` | match (reason wording, incident 9) |
| Link URLs must use the base origin | `https://sentry.io` link from loopback → error | match |
| 400/401/403/404/500 single attempt; typed `Problem` | As predicted | match |
| 429/502/503/504 retried, max 3, backoff from 100ms, Retry-After minimum | 502: 3 attempts, waits `[100ms, 200ms]`; 429 `Retry-After: 1`: wait ≥1s; 503 retried inside a page | match |
| Exhausted retries return the last `Provider` error | True for 502. A Retry-After past the deadline gives `RetryStopped` (no class, no document) instead | **not predicted** (incident 6) |
| Unreadable error document keeps the status | 502 HTML: status 502, `document` is `Err` | match |
| Bad 2xx bodies → `Concord` with `Decode`/`Metadata`/`Limit` | As predicted | match |
| Invalid quota header on 2xx → `RateMetadata` | As predicted | match |
| Debug/Display withhold token, cursor, filter, provider content | No leaks in any asserted output | match |
| Retry waits for a concurrency lease | Retry at 503ms, after the 400ms hold; control overlaps at 101ms | match |
| (not predicted) other callers share the 429 cooldown | The other caller's first send waited ~100ms after the 429 | additional |
| Reported `ConcurrentLimit: 1` caps local slots | Serialized (0/101/503ms); control overlaps | match |
| `Remaining: 0` + future `Reset` delays until the reset | Arrived 1.5ms after the reset second | match |
| Events by `eventID`, `entries` on detail, `group_id` = issue | As predicted; summaries have empty `entries` | match |
| Page retries not visible in `Page`; logical pages counted separately | `progress().succeeded == 3` with 4 server requests | match |

## Assistance label

`docs-only`. I read:

- `README.md` and `Cargo.toml` of `examples/sentry-sdk`.
- Generated rustdoc for `concord-sdk-sentry`, plus the Concord crates and
  `concord-example-testserver`.
- The three official Sentry pages listed above.

I did not open `src/`, `tests/` (other than my own new file), `FRICTION.md`, `PROVENANCE.md`,
`fixtures/` contents or rustdoc source pages. I listed the file names in `fixtures/` and checked
that `Cargo.lock` already had a `concord-sdk-sentry` entry. The author's `protocol` and
`workflows` tests were compiled and run as part of the package suite, not read.
