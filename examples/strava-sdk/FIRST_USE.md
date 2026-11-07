# FIRST_USE — fresh consumer of `concord-sdk-strava` (SDK37)

Role: fresh consumer (did not write the SDK). Date: 2026-11-07 (UTC).
Live provider calls: **NOT_RUN** — every call goes to `examples/testserver` on loopback.

## What was read before writing code

In order:

1. `examples/strava-sdk/README.md` and `examples/strava-sdk/Cargo.toml` (dependency list only).
2. Rustdoc generated with `cargo doc --locked --offline -p concord-sdk-strava -p concord-example-testserver --no-deps`
   (rendered to text with a throwaway HTML-to-text script kept in `target/tmp/`). Pages read: crate index,
   `Session`, `Strava`, `StravaAthlete`, `StravaAthleteActivities`, `ActivityPages`, `PageMeta`, `Activity`,
   `StravaActivities(Get)`, `Response`, `StravaUploads(Create|Status)`, `UploadFile`, `UploadStatus`,
   `PollPolicy`, `DataType`, `StreamMultipart`, `Credentials`, `Tokens`, `TokenRequest`, `OAuth`,
   `OAuthOauth(Token)`, `TokenPolicy`, `transport`, `StravaConfig`, `Error`, `HttpFailure`, `ErrorDocument`,
   `Problem(Field)`, `RateLimits`, `Quota`, `Window`, `BODY_LIMIT`; all of the testserver crate.
3. With `--no-deps` alone the payload types of `Error::Concord(Error)`, `Error::Admission(Error)`,
   `Error::Auth(RefreshError<Error>)`, `RateLimits.overall: Result<Option<Quota>, FieldError>` and
   `PageMeta.meta: ResponseMeta` render as unlinked bare names, so I also generated Concord's public docs
   (`cargo doc --locked --offline -p concord -p concord-core -p concord-pagination -p concord-ratelimit
   -p concord-auth -p concord-retry --no-deps`) and re-documented the SDK so the links resolve. Read:
   `concord::Error`, `ErrorKind`, `ResponseMeta`, `FieldError`, `concord_pagination::{Pages, PagesError,
   Page, Resume, Incomplete, Checkpoint}`, `concord_ratelimit::Error`, `concord_auth::RefreshError`.
4. Official Strava documentation, fetched with `curl` on 2026-11-07 06:30 UTC into `target/tmp/docs/`:
   - https://developers.strava.com/docs/authentication/
   - https://developers.strava.com/docs/rate-limits/
   - https://developers.strava.com/docs/uploads/

Not read: `examples/strava-sdk/src`, `tests/`, `fixtures/`, `FRICTION.md`, `PROVENANCE.md`, `VALIDATION.md`.

## Predictions

Common set-up for both workflows (my plan, from the README):

- `Credentials::new(client_id: u64, secret: String)?` (no I/O), a `Tokens { .. }` literal (all fields are
  public; there is no constructor), `Session::connect(server.url(), server.url(), credentials, tokens,
  timeout)?`, then `session.client()` for the generated `Strava` client.
- `Session::connect` takes an **origin** (`https://www.strava.com`), while the endpoint docs say the declared
  base is `https://www.strava.com/api/v3`. I predict the SDK appends `/api/v3` itself for API traffic and
  uses the OAuth origin as-is, so API paths on the wire start with `/api/v3/` and the token path is
  `/oauth/token` (the docs' prose says `POST https://www.strava.com/oauth/token`; their curl example uses
  `/api/v3/oauth/token` — the SDK docs say `/oauth/token`).
- Every API request carries `authorization: Bearer <access_token>` and `accept: application/json` (Json's
  `Decode::ACCEPT`); no `referer`; redirects are not followed.
- Rate headers (`X-RateLimit-Limit/Usage`, `X-ReadRateLimit-Limit/Usage`, "short,daily") are parsed into
  `Response.rates` / `PageMeta.rates` / `HttpFailure.rates` as `overall` and `read` = `Ok(Some(Quota{short,
  daily}))`; a missing pair is `Ok(None)`, a malformed one `Err(FieldError::Invalid)`.
- Debug/Display of `Credentials`, `Tokens`, `Session`, `Error`, `UploadStatus`, `Problem` never contain the
  client secret, access/refresh token, code or provider message text.

### OAuth (prerequisite)

- `OAuth::new(transport(timeout)?).with_base_url(server.url())?.oauth().token(TokenRequest::authorization_code(&credentials, code)?).send().await?`
- Wire: `POST /oauth/token`, no query, `content-type: application/x-www-form-urlencoded`, body pairs
  `client_id=…&client_secret=…&code=…&grant_type=authorization_code` (order unknown), no `authorization`
  header. Returns `Response<Tokens>`; `attempts == 1`.
- Fails: 400/401 with a `{message, errors}` body → `Error::Http` with `ErrorDocument::Problem`; never retried.
- Expired stored token (I will set `expires_at` in the past **but leave a stale `expires_in = 21600`**, as a
  reloaded database row would have): before the first API call the session sends `POST /oauth/token` with
  `grant_type=refresh_token&refresh_token=<old>&client_id&client_secret`, then the API request with the new
  bearer; `latest_tokens().await.refresh_token` is the rotated one. I predict `expires_at` governs, not the
  stale `expires_in`.
- 401 from an API call → `Error::Http` status 401, no replay and no immediate refresh; the *next* call
  refreshes first.

### Workflow 1 — page the athlete's activities, then retrieve one

Calls:

1. `api.athlete().pages(2, Resume::Start, NonZeroUsize::new(10).unwrap())?` then `next_page().await` in a
   loop. Small page size so fixtures can show full, short and empty pages.
2. `api.activities().get(id).send().await?` for one id seen on a page.

Wire:

- Page n: `GET /api/v3/athlete/activities?page=<n>&per_page=2` (declared order `page, per_page, before,
  after`; `before`/`after` absent). Page numbering starts at 1 for `Resume::Start`.
- Detail: `GET /api/v3/activities/<id>?include_all_efforts=false` — the setter doc says "Initially false",
  so I predict the default is *sent* rather than omitted.

Returns:

- Pages: with replies [2 items], [1 item], [] the traversal makes exactly three requests. `next_page`
  returns `Some(page 1)`, `Some(page 2)`, then I am **unsure** whether the empty page 3 is surfaced as
  `Some(empty page)` followed by `None`, or directly `None`. Pagination docs say `Ok(None)` comes only
  after the provider's last page "was returned", so I predict `Some(empty)` then `None`.
  `page.meta.attempts == 1`, `page.meta.meta.status() == 200`, `page.meta.rates` parsed.
- Detail: `Response<Activity>` with `value.id == id`, `attempts == 1`, `rates` parsed. `Activity.id` is a
  `u64` (64-bit ids beyond `u32` decode).

Failures:

- Page cap reached while the last page was full → `Err(PagesError::Bounded { resume: Resume::At(n+1), .. })`;
  afterwards `Err(PagesError::Ended { .. })`.
- A provider 500 on page 2 → `Err(PagesError::Provider { error: Error::Http(500), failed: Resume::At(2), .. })`,
  no retry (only 502–504 are retried). `collect_complete()` → `Incomplete { collected: [page 1], error }`.
- A 502 on a page → retried once after 25 ms; `meta.attempts == 2` if the second attempt succeeds.
- `per_page` 0 or 201 → `Error::Invalid` from `pages(..)` with nothing sent.
- Detail 404 with `{"message":"Record Not Found","errors":[{"resource":"Activity","field":"id","code":"not found"}]}`
  → `Error::Http`, `status() == Some(404)`, `document == Ok(ErrorDocument::Problem(..))`, one request.
- 200 with a malformed JSON body → `Error::Concord(e)` with `e.kind() == ErrorKind::Decode`, one request.
- 200 with `content-type: text/html` → `Error::Concord` kind `Metadata`.
- 429 on a read with the read short window exhausted (`X-ReadRateLimit-Usage: 100,…` vs limit `100,…`) →
  `Error::Http` 429 with `rates.read` showing the exhausted window, not retried. The next read is refused
  locally: `Error::Admission(concord_ratelimit::Error::Gated(Some(d)))` with `d` ≤ 15 minutes, nothing sent.
  An upload POST (overall windows only) is **still admitted** because the overall windows are not exhausted.
- 429 with the overall *daily* window exhausted → later reads and uploads are both gated (until midnight UTC).
- 429 with no counters and no `Retry-After` → gated "until next UTC day" for everything.

### Workflow 2 — upload an activity file and poll its upload ID

Calls:

1. `UploadFile::new("morning-ride.gpx", DataType::Gpx, Body::wrap_stream(stream))?.external_id(..).name(..)`
2. `api.uploads().create(file).send().await?` → keep the receipt `Response<UploadStatus>`.
3. `api.uploads().poll(receipt.value, PollPolicy { max_polls, interval: 1s }).await?`
   (`upload_and_poll` for the one-call variant).
4. `api.activities().get(status.activity_id.unwrap()).send().await?` to show the connected result.

Wire:

- `POST /api/v3/uploads`, `content-type: multipart/form-data; boundary=…`, body sent with
  `transfer-encoding: chunked` (stream of unknown length), parts `data_type=gpx`, `external_id`, `name`, and
  `file` with `filename="morning-ride.gpx"` carrying the GPX bytes. Bearer header as above.
- Each poll: wait ≥ 1 s, then `GET /api/v3/uploads/<upload id>` (no query).

Returns:

- Create: 201 + `{"id":…, "id_str":"…", "external_id":"…", "error":null, "status":"Your activity is still
  being processed.", "activity_id":null}` → `Response<UploadStatus>` with `activity_id == None`.
- Poll: returns `Ok(UploadStatus)` on the first status with `activity_id: Some(..)`; number of GETs equals
  the number of replies until ready.

Failures:

- Processing error (`"error": "morning-ride.gpx duplicate of activity 123"`) →
  `Err(Error::Processing(status))` with the status (id, error text) retained; no more polls.
- Poll cap reached while still processing → `Err(Error::PollBounded(last))`; resume with `poll(*last, ..)`.
- `interval` < 1 s → `Error::Invalid` before any request; for `upload_and_poll` I predict validation happens
  before the POST (nothing sent).
- Filename with a quote/control character → `Error::Invalid` from `UploadFile::new`, source never polled.
- Response slower than the session timeout → `Error::Ambiguous`, `is_ambiguous() == true`, exactly one POST
  observed by the server, no GET, no replay.
- Stream source erroring mid-body → `Error::Ambiguous` too (no head arrived).
- Upload 500 → `Error::Http` with `ambiguous == true` and `is_ambiguous() == true`, one POST, no retry
  (uploads are not in the 502–504 retry set). Upload 502 → also one POST (one-shot body cannot be replayed).
- Upload 400 with an upload-status-shaped body → `Error::Http` 400 with `document ==
  Ok(ErrorDocument::Upload(status))`.
- 201 with a malformed receipt → ambiguous (`Error::Ambiguous` or `is_ambiguous()`); no polling.
- 429 on the upload with the overall short window exhausted → `Error::Http` 429; a following *read* is gated
  too (reads check overall + read windows).

---

Everything below was written after the predictions above, from observed runs.

## First working call

`first_working_call` in `tests/first_use.rs` compiled and passed on the first attempt (no source lookup).
It took: `Credentials::new(id, secret)?`, a six-field `Tokens { .. }` struct literal (no constructor, so
`token_type`, `expires_in` and `scope` must be invented for a reloaded token), `Session::connect(&url, &url,
credentials, tokens, timeout)?`, then `session.client().activities().get(id).send().await?`. Imports:
`Credentials`, `Session`, `Tokens` — nothing I did not use. Observed wire:
`GET /api/v3/activities/16123456789?include_all_efforts=false` with `accept: application/json`,
`authorization: Bearer …`, `host`, and nothing else.

## The application (`tests/first_use.rs`, 25 tests)

| Test | Caller task / failure exercised |
|---|---|
| `first_working_call` | one detail GET |
| `w1_authorize_page_activities_then_get_one` | code exchange → session → `pages(2, Start, cap 10)` → `activities().get` ; wire for all 5 requests; rate quotas on every page; secrets absent from `Debug` |
| `w1_reloaded_expired_tokens_refresh_before_the_first_page` | stored row with past `expires_at`, stale `expires_in` → refresh form, new bearer, `latest_tokens()` rotated |
| `w1_page_cap_reports_resume_and_resumes` | cap → `Bounded { resume: At(2) }` → `Ended`; new traversal from the resume key |
| `w1_failed_page_keeps_prefix_and_transient_page_is_retried` | 500 on page 2 → `Incomplete` with the prefix, `failed`/resume `At(2)`, not retried; 502 → `attempts == 2` |
| `w1_detail_failures` | 404 problem document; truncated JSON → `Decode`; `text/html` → `Metadata`; 403 non-JSON → status kept, document `Err`; 1.1 MB body → `Limit`; 5 requests, no retries |
| `w1_unauthorized_then_next_call_refreshes` | 401 → no replay; next call: refresh then GET with the new bearer |
| `w1_invalid_page_size_sends_nothing` | `per_page` 0 / 201 → `Error::Invalid`, nothing sent |
| `w1_incremental_sync_after_last_sync_uses_the_single_page_endpoint` | "activities after my last sync": hand-written loop over `athlete().activities().set_after(..)` |
| `w1_concurrent_calls_share_one_refresh` | two clones, expired token, `join!` → one `/oauth/token` |
| `w1_read_without_head_is_retried_once_upload_is_not` | hang-up before head: GET `attempts == 2`; POST → `Error::Ambiguous`, one POST |
| `w429_read_window_gates_reads_but_not_uploads` | 429 with the read short window exhausted → next read `Admission(Gated(Some(≤15 min)))`, upload still sent |
| `w429_overall_daily_window_gates_reads_and_uploads` | 429 on the upload with overall daily exhausted → read and upload both gated (via a second client of the same session) |
| `w429_overall_short_window_on_upload_gates_reads` | 429 on the upload with overall short exhausted → next read `Gated(Some(≤15 min))` |
| `w429_unexplained_gates_everything` | 429 without counters or `Retry-After` → next upload gated |
| `w429_usage_headers_are_observed_per_class` | partial and malformed pairs → `Err(FieldError::Invalid)` per class; a 200 showing read usage 100/100 gates the next read, not an upload |
| `w429_retry_after_is_a_global_cooldown` | 429 + `Retry-After: 30` with room in all windows → reads and uploads gated for 20–30 s |
| `w2_upload_keep_receipt_poll_until_ready_then_read_activity` | `create(file).send()` → receipt (201) → `poll(receipt, 1 s)` → ready after 2 GETs (≥ 2 s) → `activities().get(activity_id)`; multipart parts, chunked one-shot body, file bytes unchanged |
| `w2_upload_and_poll_reports_processing_error` | duplicate error (HTML in text) → `Error::Processing(status)`; text withheld from `Debug`/`Display` |
| `w2_poll_bound_then_resume_with_retained_receipt` | `max_polls 1` → `PollBounded(last)` → `poll(*last, ..)` completes; POST, GET, GET only |
| `w2_upload_timeout_is_ambiguous_and_never_replayed` | 400 ms session timeout, 3 s server delay → `Error::Ambiguous`, `status() None`, one POST, no GET after 1.5 s more |
| `w2_source_failure_is_ambiguous_and_never_replayed` | file stream errors mid-body → `Error::Ambiguous`; one connection, no completed request |
| `w2_upload_server_errors_are_ambiguous_and_not_retried` | 500/502/503 → `Error::Http` with `ambiguous == true`, one POST each |
| `w2_upload_rejection_and_unreadable_receipt` | 400 with upload schema → `ErrorDocument::Upload`; truncated 201 receipt → `Error::Ambiguous`, `status() == Some(201)`, no polling |
| `w2_invalid_inputs_send_nothing` | filename with `"` → `Error::Invalid`, source never polled; 200 ms poll interval → `Error::Invalid` before the POST |

Results (observed, local, 2026-11-07):

- `cargo test --locked --offline -p concord-sdk-strava --test first_use`: 25 passed, 0 failed (~2.0 s);
  repeated 4 times, stable.
- `cargo test --locked --offline -p concord-sdk-strava` (whole package, author's tests run, not read):
  unit 0, `first_use` 25, `protocol` 12, `workflows` 10, doctests 3 — all passed.
- `cargo clippy --locked --offline -p concord-sdk-strava --all-targets -- -D warnings`: clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-strava --no-deps`: clean.
- `rustfmt --check` on `tests/first_use.rs`: clean (only my file was formatted; `cargo fmt` was not run
  on the package so the SDK's `src/` was not touched).
- `Cargo.lock` and the root `Cargo.toml` were not changed: the package was already a workspace member and
  in the lock at the start, so `--locked` worked from the first build. No new crate.
- Live provider calls: NOT_RUN.

## Friction incidents, in order

1. **`--no-deps` rustdoc hides which `Error` is which.** `Error::Concord(Error)`, `Error::Admission(Error)`
   and `Error::Auth(RefreshError<Error>)` render as three identical unlinked `Error`s, and
   `RateLimits.overall: Result<Option<Quota>, FieldError>`, `PageMeta.meta: ResponseMeta`,
   `ActivityPages = Pages<..>` are unlinked too. The README's own verification command is `--no-deps`.
   Resolution: document `concord`, `concord-core`, `concord-pagination`, `concord-ratelimit`,
   `concord-auth`, `concord-retry` and re-document the SDK. Cost: one extra doc build and reading three
   more crates. Suggestion (SDK docs, not done): name the payload types in the variant docs
   (`concord::Error`, `concord_ratelimit::Error`).
2. **`OAuthOauth` / `OAuthOauthToken` and `oauth.oauth().token(..)`.** The generated scope type names stutter
   (client `OAuth`, scope `oauth`). Not imported, but they head the rustdoc struct list and the call chain
   reads oddly. Harmless.
3. **Two ways to point OAuth at a server.** The exchange uses the generated `OAuth::new(transport(..)?)
   .with_base_url(url)?`, while the session takes `oauth_origin` as a `connect` argument. The README shows
   the production form only; I worked it out from `with_base_url`'s doc. `Session::connect`'s doc does not
   say that `/api/v3` is appended to `origin` (I inferred it from the endpoint docs; it was right).
4. **`Tokens` for a reloaded row.** No constructor; the struct literal forces `token_type`, `expires_in`,
   `scope`. The docs do not say which of `expires_at` / `expires_in` drives refresh. Observed: `expires_at`
   (a past `expires_at` with a stale `expires_in = 21600` refreshed before dispatch). Worth one sentence on
   `Tokens` or `Session::connect`.
5. **`Error::Admission` / `Error::Auth` payloads come from crates the SDK does not re-export.** The SDK
   re-exports `concord`, `pagination` and `retry` (I never needed `retry`), but the only way to read the gate
   delay is `Error::Admission(concord_ratelimit::Error::Gated(Some(wait)))`. My in-repo test compiled only
   because integration tests can name the package's normal dependencies; a downstream application would have
   to add `concord-ratelimit` itself at a matching version. Suggestion: re-export `concord_ratelimit` (and
   `concord_auth`) or offer `Error::retry_after() -> Option<Duration>`.
6. **`pages` has no filters, but its docs talk about filters.** `StravaAthlete::pages(per_page, from,
   max_pages)` takes no `before`/`after`, yet its doc says "Keep the same filters and page size on resume"
   and the README says "Resuming requires the same athlete, filters and page size". The everyday task
   "activities after my last sync" therefore drops to the single-page endpoint with a hand-written loop,
   end test and cap (`w1_incremental_sync_…`), losing `Bounded`/`Ended`/resume reporting and the page-meta
   shape. This was the largest usage gap I hit. Suggestion: `pages` (or a builder) accepting `after`/`before`.
7. **My own test bug, from the end-of-listing rule.** After a short page I asserted `next_page() == None`;
   the SDK correctly fetched one more page and surfaced it as `Some(empty page)` first. The README states the
   rule ("continues until an empty page, including after a short page") and I had predicted it for the
   first traversal; I then forgot it in a second one. Fixed the test, not the SDK. Consequence worth knowing:
   every complete traversal costs one extra request against the read quota, and callers see a 0-item page.
8. **`page.meta.meta.status()`.** `PageMeta.meta` (the HTTP head) inside `page.meta` stutters; `Response.meta`
   and `HttpFailure.meta` are the same type with the same name, which at least is consistent.
9. **Three spellings of "ambiguous".** `Error::Ambiguous(_)` (timeout, hang-up, source failure, unreadable 201),
   `HttpFailure.ambiguous == true` inside `Error::Http` (upload 5xx), and `Error::is_ambiguous()` covering both.
   The README table says "Ambiguous" for both rows, which reads like the variant; only `is_ambiguous()` is the
   safe single check. One sentence in the README would settle it.
10. **Inspecting an HTTP failure needs a helper.** Every assertion on a rejection went through
    `match err { Error::Http(f) => f, .. }` then `match &f.document { Ok(ErrorDocument::Problem(p)) .. }`;
    I wrote a local `http(&Error) -> &HttpFailure`. `Error::status()` covers the common case; an
    `Error::http(&self) -> Option<&HttpFailure>` accessor would remove the helper.
11. **One-second poll floor in tests.** The poll interval cannot go below 1 s (`Error::Invalid`, good for
    production), and there is no injectable clock, so every polling test costs ≥ 1 s per poll. The
    `PollPolicy` doc says "Production polls wait at least one second", which suggests a non-production mode
    that does not exist publicly.
12. **Upload options vs the provider docs.** `UploadFile` offers `name`, `sport_type`, `external_id`; the
    Strava uploads page also documents `description`, `trainer`, `commute`. Stated in the README as the
    supported set; noted as a caller gap, not a defect in this slice.

Not friction (checked): untyped integer literals work for `get(404)`, `status(13987654321)`,
`set_after(1762000000)` (probed in a throw-away test file, then deleted); `?` from `PagesError` into
`Box<dyn Error + Send + Sync>` works; no unused imports were needed; no diagnostic was misleading; no source
lookup was needed.

## Predictions vs observed

| Prediction | Observed |
|---|---|
| API paths under `/api/v3/`, token at `/oauth/token` | matched |
| `authorization: Bearer`, `accept: application/json`, no `referer` | matched (headers were exactly accept, authorization, host) |
| Code exchange: form body with 4 pairs, no `authorization`, `attempts == 1` | matched |
| Expired reloaded token → refresh first; `expires_at` governs over stale `expires_in` | matched |
| 401 → no replay, next call refreshes | matched |
| Page query `page=<n>&per_page=<s>`, `before`/`after` absent, page 1 for `Start` | matched; with `set_after` the order is `page, per_page, after` |
| Detail sends `include_all_efforts=false` | matched |
| Pages [2],[1],[] → three requests, `Some, Some, Some(empty), None` | matched (I was unsure; my second traversal's assertion contradicted it — incident 7) |
| Cap → `Bounded { At(n+1) }`, then `Ended` | matched; the cap is checked before sending, so no extra request |
| 500 on a page → `Provider`, not retried, `collect_complete` keeps the prefix | matched |
| 502 → one retry, `attempts == 2` | matched; a hang-up before the head is retried the same way |
| `per_page` 0 / 201 → `Error::Invalid`, nothing sent | matched |
| 404 → `Error::Http` with `ErrorDocument::Problem` | matched |
| Malformed JSON → `Decode`; `text/html` → `Metadata` | matched; oversize → `Limit` as documented |
| 429 read-window → reads gated ≤ 15 min, uploads admitted | matched |
| 429 overall daily → reads and uploads gated | matched |
| 429 without counters or hint → gated | matched (`Admission`) |
| Missing pair `Ok(None)`, malformed `Err(Invalid)` | matched; partial (limit without usage) is also `Err(Invalid)` — I had no prediction for partial |
| (not predicted) `Retry-After: 30` with room in all windows | global cooldown of 20–30 s for reads and uploads |
| Multipart, chunked, parts `data_type`, `external_id`, `name`, `file; filename=` | matched; no `sport_type` part unless set |
| Create returns the receipt without polling; poll waits ≥ 1 s per GET | matched |
| Processing error → `Error::Processing(status)`; cap → `PollBounded(last)`, resumable | matched |
| Poll interval < 1 s → `Error::Invalid` before the POST | matched |
| Bad filename → `Error::Invalid`, source not polled | matched |
| Timeout → `Error::Ambiguous`, one POST, no GET | matched |
| Source error mid-body → `Error::Ambiguous` | matched (server saw one connection and no completed request) |
| Upload 5xx → `Error::Http` with `ambiguous`, one POST (502 too) | matched for 500, 502, 503 |
| Upload 400 upload-schema → `ErrorDocument::Upload` | matched |
| Unreadable 201 receipt → ambiguous, no polling | matched; precisely `Error::Ambiguous` with `status() == Some(201)` |
| Overall-short 429 on upload gates reads | matched (`Gated(Some(≤ 15 min))`) |
| Secrets/provider text absent from `Debug`/`Display` | matched for tokens, credentials, session, response, 404 problem, processing error |

No SDK behaviour contradicted a prediction. The one failing run was my own assertion (incident 7).

## Assistance label

`docs-only`: `examples/strava-sdk/README.md`, the generated rustdoc of `concord-sdk-strava` and
`concord-example-testserver`, Concord's public rustdoc (`concord`, `concord-core`, `concord-pagination`,
`concord-ratelimit`, `concord-auth`, `concord-retry`), and the three official Strava pages listed at the top
(read 2026-11-07). Not opened at any point: `examples/strava-sdk/src`, `tests/protocol.rs`,
`tests/workflows.rs`, `tests/support`, `fixtures/`, `FRICTION.md`, `PROVENANCE.md`, `VALIDATION.md`, any
other example, crate source or campaign file. The author's tests were run as part of the package test
command but not read.
