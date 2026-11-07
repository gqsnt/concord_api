# First use — SDK27 Mailgun (fresh consumer)

Role: fresh consumer of `concord-sdk-mailgun` at commit
`80cd6cf47744ebcb4b43d6a72f4314e6ff080d21`. Live provider calls: **NOT_RUN**. Every
call below ran against `concord-example-testserver` on 127.0.0.1.

## Files read before writing any code

1. `examples/mailgun-sdk/README.md`.
2. `examples/mailgun-sdk/Cargo.toml`. I read it to learn which crates a test in this
   package can name. This is a manifest, not source.
3. Generated rustdoc (`cargo doc --locked -p concord-sdk-mailgun --no-deps`). I converted
   each page to text with a local HTML-to-text script. I read every item page.
4. Concord public rustdoc (`cargo doc --locked -p concord -p concord-pagination
   -p concord-example-testserver --no-deps`). I read the `Pages`, `PageItems`,
   `PagesError`, `Incomplete`, `Resume`, `Page` pages, the `concord::Error`, `ErrorKind`,
   `ResponseMeta` and `Operation` pages, the facade's re-exports, and the whole
   test-server crate.
5. Directory listings only (no content): `examples/mailgun-sdk/{src,tests,fixtures}`.

Not read: `examples/mailgun-sdk/src`, `tests/`, `fixtures/*`, `FRICTION.md`,
`PROVENANCE.md`, `VALIDATION.md`. I wrote my own fixtures from the official pages.

## Official pages read (2026-11-07, read-only, via a fetch-and-summarize tool)

- https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-http:
  the US base is `https://api.mailgun.net`, the EU base is `https://api.eu.mailgun.net`,
  Basic `api:KEY`, and the message size limit is 25 MB. It says an "Accepted" event is
  logged on acceptance and a "Delivered" event if delivery succeeds.
- https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/messages/post-v3--domain-name--messages:
  `POST /v3/{domain_name}/messages`, multipart/form-data, basicAuth. The 200 body has
  `id` and `message`. 400, 429 and 500 have a `{"message"}` body; 401 has no documented
  body. The page states no general send idempotency.
- https://documentation.mailgun.com/docs/mailgun/user-manual/events/events: follow the
  next URL until an empty page. Next and previous URLs are always returned. Retention
  is 30 days. `accepted` means queued, `delivered` means the recipient server accepted
  the message.
- https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/events/get-v3-domain_name-events:
  `begin` and `end` are in epoch seconds, `ascending` is yes/no, `limit` is at most 300,
  and filters include `event`, `recipient` and `message-id`. `paging.next` and
  `paging.previous` are strings. `severity` is temporary or permanent. `flags` has
  `is-test-mode`. **This page describes `items.timestamp` as "Unix epoch, in
  nanoseconds".** It lists no top-level `recipient` item field. The SDK's `Event`
  rustdoc says the manual describes seconds. I use fractional seconds in fixtures, as
  real Mailgun payloads do, and I record the documentation conflict as an observation.

## Predictions

These were written before any test code. They come from the README and rustdoc only.

### Common

- `Mailgun::local(server.url(), key, timeout)` does no I/O. It refuses a non-loopback
  or HTTPS origin with `Error::Input(InputError::BaseUrl)` and a zero timeout with
  `InputError::Timeout`.
- Every request carries `authorization: Basic base64("api:" + key)`. That is
  `Basic YXBpOmtleS1zeW50aGV0aWMtMGYxZTJkM2M0YjVhNjk3ODg3OTZhNWI0YzNkMmUxZjA=` for my
  synthetic key. I also expect `accept: application/json`, because the client
  response format is Json.
- `send()` returns `Result<Response<T>, Error>` through the client's `SendPolicy`.
  `Response.meta.status()` is the HTTP status. `meta.rate_limit()`,
  `meta.rate_remaining()` and `meta.rate_reset_millis()` read Mailgun rate headers. I
  guess these are `x-ratelimit-limit`, `x-ratelimit-remaining` and `x-ratelimit-reset`;
  the rustdoc does not name them. `retry_after(now)` parses `Retry-After`.
- Secrets: the `Debug` output of `Error`, `Response`, `Accepted`, `MessageId`, `Event`,
  `Paging`, `EventCursor` and `Incomplete` contains no key, message ID, recipient or
  URL. `Display` of `Error` contains none of them either.

### Workflow 1 — send a text email with one attachment and keep the ID

Calls: `Domain::new("mg.example.test")?`,
`Attachment::bytes("report.txt", "text/plain", bytes)?` (or `from_file` / `stream`),
`TextEmail::new(from, vec![to…], subject, text, attachment)?`, then
`api.domain(domain).send_message(email).send().await?`.

Wire: `POST /v3/mg.example.test/messages` with no query and
`content-type: multipart/form-data; boundary=…`. The parts are `from`, `to` (one part
per recipient), `subject`, `text` and `attachment`
(`filename="report.txt"`, `Content-Type: text/plain`, the exact bytes). A buffered
attachment sends a `content-length` body. A `stream` attachment sends a chunked body.

Returns: `Response<Accepted>`, where `value.id` is a `MessageId` whose `as_str()` is the
exact provider string with angle brackets
(`<20261107090000.1.4F2A8C@mg.example.test>`) and `value.message` is
`"Queued. Thank you."`. `Display` of the ID is its query spelling, which I guess drops
the angle brackets.

Failures:
- 400 with `{"message": "..."}` gives `Error::Provider(p)` with
  `p.class() == HttpClass::BadRequest` and `p.document == Ok(Problem { message })`.
- 401 with a plain-text body (no JSON documented) gives `Provider`, class
  `Unauthorized`, and `document` is `Err(..)` while the status is kept.
- 429 gives `Provider`, class `RateLimited`. `retry_after` reads the hint. The SDK makes
  no second request.
- 5xx gives `Provider`, class `Server`, with exactly one request.
- The connection dropped after the request was read, a timeout before the response
  head, a stalled 200 body, and a malformed 200 body each give `Error::AmbiguousSend`.
  The server sees exactly one request and no replay arrives later.
- A stream attachment whose source fails after its first chunk gives
  `Error::AmbiguousSend`, with exactly one request. The source is polled zero times by
  construction and `prepare()`.
- `TextEmail` has no `Clone`, so a replay needs a new value, which `compile_fail`
  documents.
- Local input refusals: zero recipients and a filename with `/` each give
  `Error::Input(..)` before any I/O.

### Workflow 2 — search and page events, then inspect delivery

Calls: `domain.events(EventRange::new(begin, end)?)` with
`.set_message_id(id.clone())`, `.set_recipient("alice@example.test")` and
`.set_limit(PageSize::new(2)?)`, then `.pages(NonZeroUsize::new(10).unwrap())?`,
`.items()`, `.collect_complete().await`. Finally
`delivery_status(&id, "alice@example.test", &events)`.

Wire, first page: `GET /v3/mg.example.test/events?…` with `begin=1794042000`,
`end=1794045600`, `ascending=yes`, `limit=2`,
`message-id=20261107090000.1.4F2A8C@mg.example.test` (no angle brackets) and
`recipient=alice@example.test`. The order is not documented. I guess the range pairs
come first. Later pages: `GET` of the `paging.next` path, verbatim, with no query
appended, the same `authorization` and the same host.

Returns: `Vec<Event>` in provider order across pages. The traversal ends on the first
page with empty `items`, and that page counts toward the cap.
`delivery_status(..)` returns `DeliveryStatus::Delivered` when the latest relevant
event for that ID and recipient is `delivered`. It returns `Accepted` when only
`accepted` is seen, `TemporaryFailure` for `failed` + `temporary`, `PermanentFailure`
for `failed` + `permanent`, `TestMode` for a delivered event flagged test mode, and
`Unknown` when no event matches.

Failures:
- A 5xx on page 2 gives `Err(Incomplete)` with `collected` holding page 1's items. The
  error is `PagesError::Provider { error: Error::Provider(..), failed: Resume::At(cursor),
  .. }`. `incomplete.error.resume()` is `Some(At(cursor))`.
  `search.clone().pages_from(resume.clone(), cap)` requests exactly the page-2 URL
  again, and only when I call it.
- A `paging.next` on another origin, or for another domain's events, gives
  `Error::Link(LinkError::Untrusted)` inside `Incomplete`, with no request sent for it.
- Hitting the cap while a next link exists gives `PagesError::Bounded` with a resume
  position.
- A malformed page body, or a missing `paging`, gives `Error::Concord(e)` with
  `e.kind() == ErrorKind::Decode`.
- A 401 on events (a sending key) gives `Provider`, class `Unauthorized`, inside
  `Incomplete` with an empty prefix.
- Reads are not retried: one request per page.

---

Everything below was written after the tests ran. Evidence labels: **observed** means
seen on the loopback fixture. **Inferred** means a reasoned claim about the real
provider that I did not verify (live calls NOT_RUN).

## First working call

What it took: the README and the rustdoc pages for `Mailgun`, `MailgunDomain`,
`MailgunDomainSendMessage`, `Attachment`, `TextEmail` and `Accepted`. I wrote one
30-line test that copied the README caller's shape, swapping `from_file` for
`Attachment::bytes`. It needed only crate-root imports plus the test server. It
compiled with zero errors and passed on its first run (about 0.5 s incremental build).
Its printout showed the whole wire request:

- `POST /v3/mg.example.test/messages`
- `authorization: Basic …`, `content-type: multipart/form-data; boundary=concord-boundary-0`,
  `accept: application/json` and `content-length`
- parts in the order `from`, `to`, `subject`, `text`, then
  `attachment; filename="report.txt"` with `Content-Type: text/plain`.

I then replaced it with the application in `tests/first_use.rs`.

## Application (`tests/first_use.rs`, 15 tests, all against `TestServer`)

Task 1 (send and keep the ID):
- `send_report_with_file_attachment_and_keep_message_id`: `from_file` with two
  recipients. Asserts every multipart part, the exact Basic header, the
  `content-length` body, the ID round-tripping through storage, the rate headers,
  redaction and no replay.
- `attachment_that_contains_the_boundary_text_is_framed_correctly`
- `provider_statuses_are_typed_and_never_replayed`: 400 JSON, 401 text, 429 with
  `Retry-After`, 503 HTML, 302.
- `ambiguous_sends_are_reported_and_never_replayed`: hang-up, head timeout, stalled 200
  body, truncated JSON, missing `id`.
- `streamed_attachment_is_sent_once_and_never_rewound`: a successful chunked send, zero
  polls at construction and at `prepare`, a source failing mid-body, and a 503 on a
  streamed send.
- `local_refusals_happen_before_any_io`
- `regional_clients_target_the_regional_origin_without_io`

Task 2 (page events, inspect delivery):
- `send_then_page_events_and_inspect_delivery`: the connected workflow. It sends, keeps
  the ID, follows 3 pages and inspects delivery.
- `failed_page_keeps_prefix_and_resumes_only_when_asked`
- `resume_position_refuses_a_different_search`
- `untrusted_next_links_are_refused_before_dispatch`: a real-provider origin, another
  domain, and userinfo in the link.
- `a_next_link_back_to_a_fetched_page_is_a_cycle`
- `page_cap_and_bad_pages_are_not_completion`: the cap, a missing `paging`, and 401.
- `page_metadata_is_available_page_by_page`
- `delivery_interpretation_does_not_overclaim`

Ordinary path used: `Mailgun::local` → `.domain()` → `.send_message(email).send()` /
`.events(range).set_*().pages(cap)?.items().collect_complete()` → `delivery_status`.

Advanced APIs, each used only where a task needed it:
- `pages_from`, for explicit resumption.
- `Pages::next_page`, for per-page `Metadata`.
- `prepare().request()`, only to inspect the EU URL without I/O.
- `serde_json` decoding of `EventPage`, only to feed `delivery_status` variants without
  a server.

Extra imports beyond the crate root: `concord::ErrorKind`, `concord::bytes::Bytes`,
`pagination::{PagesError, Resume}`, and `futures_util::StreamExt` (only for my poll
counter).

## Friction incidents, in order

1. **Unlinked types in rustdoc (doc lookup).** The signatures of `Attachment::stream`
   and `MailgunDomainEvents::pages_from` render `Bytes`, `Stream` and `Resume` without
   links under `--no-deps`.
   - The stream error type was readable only from the README and an HTML `title`
     attribute (`core::io::error::Error`).
   - To name `Bytes` I searched the `concord` facade docs and found
     `concord_sdk_mailgun::concord::bytes::Bytes`. The SDK root does not re-export it.
   - Building a stream needs the caller's own `futures-util` (or similar) dependency.
   - Cost: about three doc lookups.
2. **Range query pairs are not in the generated endpoint doc.** The `events` doc says
   "Query (when present): limit, message-id, recipient, event". The required
   `begin`/`end`/`ascending=yes` pairs come from `EventRange: QueryPairs`, and their
   wire names are learnable only by observation. This is a Concord generated-doc gap
   for `QueryPairs` parameters (a candidate macro-doc change, not edited).
3. **README wording misled me.** "`MessageId::Display` is deliberately its query wire
   spelling" made me predict a bracket-less query value. Observed: `Display` ==
   `as_str()` == `<…@mg.example.test>`. A clearer doc would say "identical to
   `as_str()`".
4. **Unverified provider contract (inferred risk).** The search sends
   `message-id=%3C…%3E`, with brackets. The official reference says the filter takes
   the ID "as returned by the Messages API", which supports this. Events, however,
   carry the bare ID in `message.headers.message-id`. Whether Mailgun's filter matches
   the bracketed form is **not verified** (live NOT_RUN). If it does not match, the
   connected workflow would return zero events and `Unknown`, with no error.
5. **Rate-limit header names are undocumented.** `Metadata::rate_limit`,
   `rate_remaining` and `rate_reset_millis` do not name the header fields they read. I
   guessed `x-ratelimit-limit/-remaining/-reset`, and the guess held on the fixture.
6. **Misleading compile error.** `assert_eq!` on `Option<&Resume<EventCursor>>` and on
   `Option<Page<…>>` failed with E0369, because `EventCursor` has no `PartialEq`. The
   compiler note cited `examples/mailgun-sdk/src/events.rs:231`; that was the location
   only, and I did not open the file. I switched to `matches!`.
7. **Surprising name: "Provider" for local refusals.** A local link refusal
   (`Error::Link(Untrusted)` or `Cycle`, never sent) arrives as
   `PagesError::Provider { .. }` through `provider_error()`. Its Debug prints
   `Provider { failed: Start, … }`. In the pagination engine, "provider" means "the
   page factory", not Mailgun. I had to read `concord_pagination` docs to see why.
8. **Unclear resumability: untrusted links and cycles behave differently.**
   - Untrusted link: the page carrying it fails whole. Its valid item is dropped
     (`collected` is empty) and `resume` is that page (`Start`), so an explicit resume
     fails the same way again.
   - Cycle: the page carrying it is **kept** (`collected` = 2) and the duplicate request
     is refused before dispatch. Resuming refetches that page, re-emits its item (a
     duplicate) and meets the same cycle.
   - The README's "A rejected next link fails its page … not that page's items" matches
     the untrusted case but not the cycle case. Nothing says which failures are worth
     resuming. A 5xx is. A `Link(_)` never is.
9. **Completeness is not in the types.** `delivery_status` on a prefix returns
   confident variants: `TemporaryFailure` after a page-2 502, and `Delivered` from the
   cycle prefix. The docs say a prefix is partial evidence, but the caller must carry
   completeness alongside the `Vec<Event>`.
10. **Fixture observability (test server, not the SDK).** A send whose stream fails
    mid-body is not in `server.requests()`. Only `connections() == 1` and
    `request_body_bytes() > 0` show that one attempt happened.
11. **Conflicting official documentation.** The API reference describes
    `items.timestamp` as nanoseconds. The SDK rustdoc says the manual describes
    seconds, and my fixtures use fractional seconds.
    - Observed: an integer nanosecond value decodes and is kept raw.
    - The reference also lists no top-level `recipient` item field. Observed: without
      one, `delivery_status` returns `Unknown` and does not read `envelope.targets`.
      This is conservative, but it would make every conclusion `Unknown` if the real
      payload lacked the field. That is inferred, not observed.

No source lookups were needed to finish either task.

## Predictions vs observed

| Prediction | Observed | Result |
| --- | --- | --- |
| `local` does no I/O; it refuses https (`BaseUrl`) and a zero timeout (`Timeout`) | same | match |
| `authorization: Basic base64(api:key)`, `accept: application/json` on every request | same, including continuation pages | match |
| Rate headers `x-ratelimit-*`; `retry_after` parses | `Value(300/299/…)`, `Retry-After: 30` → `Some(30 s)` | match (a guess) |
| Debug/Display of errors, responses, events and `Incomplete` hide the key, credential, bare ID, recipient, cursor and host | none appeared | match |
| `POST /v3/{domain}/messages`; parts `from`, `to`×N, `subject`, `text`, `attachment` with filename and type | same; a fixed boundary `concord-boundary-0`, switched to `-1` when the attachment contains the boundary text | match (boundary not predicted) |
| Buffered → `content-length`; stream → chunked | same | match |
| `Accepted.id.as_str()` exact, with brackets; `message` kept | same | match |
| `MessageId` Display drops the brackets | Display == `as_str()`, with brackets | **mismatch** |
| 400 → `BadRequest` + `Ok(Problem)`; 401 text → `Unauthorized` + `Err` document; 429 → `RateLimited`; 5xx → `Server`; one request each | same; 302 → `Unexpected`, not followed | match |
| Hang-up, timeout, stalled 200 and malformed 200 → `AmbiguousSend`, no replay | same; inner kinds `Transport` ×3 and `Decode` ×2; Display is a fixed "outcome unknown; do not automatically replay" | match |
| Stream failure mid-body → `AmbiguousSend`, exactly one request | `AmbiguousSend`, one connection, partial body, **zero complete requests recorded** | match (the observable differs) |
| Zero polls at construction and `prepare` | 0 and 0; 2 polls for 2 chunks on send | match |
| Empty recipients, `../` filename → `Input` | `Message`, `Attachment`; a CRLF subject → `Message`; a missing file → `File` | match |
| Events query `begin`, `end`, `ascending=yes`, `limit`, bare `message-id`, `recipient` (range first) | that order; `message-id` **with brackets** | **mismatch** (ID spelling) |
| Continuations: the `paging.next` path verbatim, no query appended | same | match |
| Empty page terminates and counts | 3 GETs; the empty page's `next` was never requested | match |
| `delivery_status` → Delivered / Accepted / TemporaryFailure / PermanentFailure / TestMode / Unknown | same; `opened` after `accepted` stays `Accepted`; same-time contradiction → `Conflicting` | match |
| Page-2 5xx → `Incomplete` with page-1 prefix, `Provider{Server}`, `At(cursor)`; resume only when asked | same; the resumed request target equals the failed one byte for byte | match |
| Untrusted next → `Link(Untrusted)`, no request for it | same; the carrying page's items are dropped and resume = `Start` | match (prefix shape not predicted) |
| Cap with a next link → `Bounded` + resume | same | match |
| Missing `paging` → `Concord` `Decode` | same | match |
| Events 401 → `Provider{Unauthorized}`, empty prefix | same; resume = `Start` | match |
| (not predicted) cursor reused on another search | `Link(Context)`, returned synchronously by `pages_from` | — |
| (not predicted) cycle | the carrying page is kept; the duplicate is refused; resume repeats the cycle | — |
| Reads not retried | one request per page in every case | match |

## Commands and results (2026-11-07, this worktree, `CARGO_TARGET_DIR` absolute, `CARGO_BUILD_JOBS=4`)

- `cargo doc --locked -p concord-sdk-mailgun --no-deps` → ok
- `cargo doc --locked -p concord -p concord-pagination -p concord-example-testserver --no-deps` → ok
- `cargo test --locked -p concord-sdk-mailgun` → ok: first_use 15/15, protocol
  10/10, workflows 9/9, doctests 1 + 1. `first_use` also passed three more
  consecutive runs.
- `cargo clippy --locked -p concord-sdk-mailgun --all-targets -- -D warnings` → ok,
  after one fix in my file: type aliases for the case tables (`type_complexity`).

## What I would change (recorded, not edited)

- SDK: re-export `Bytes` at the root, or document the `concord::bytes` path, next to
  `Attachment::stream`.
- SDK: document `MessageId` Display as identical to `as_str()`.
- SDK: name the rate-limit headers.
- SDK: state that `Link(_)` failures are not resumable, and document the different
  prefix behaviour for untrusted links and cycles (or make them consistent).
- SDK: consider a completeness-carrying input for `delivery_status`.
- SDK, before any live use: confirm with Mailgun whether `message-id` must be sent
  without brackets.
- Concord generated docs: list the pairs a `QueryPairs` parameter writes, or at least
  name the parameter in the endpoint's "Query" line.
- Pagination: a variant name or doc note making clear that `PagesError::Provider`
  wraps any factory error, including local refusals.

## Assistance label

`docs-only`. Inputs were the README, the SDK manifest, the SDK rustdoc, and the
Concord, pagination and test-server rustdoc. I did not open SDK `src/`, `tests/`,
`fixtures/`, `FRICTION.md`, `PROVENANCE.md` or `VALIDATION.md`. Two things reached me
only through tool output:
- a compiler note naming `src/events.rs:231` as the location of `EventCursor`;
- the names of three `tests/protocol.rs` tests printed by `cargo test`.
