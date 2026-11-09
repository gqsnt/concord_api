# Fresh consumer: SDK38 Mastodon

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

## Evidence boundary

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All application traffic will use
`examples/testserver` loopback fixtures. SDK source, existing SDK tests, `FRICTION.md`, and
`PROVENANCE.md` have not been read. No SDK source changes are planned.

Initial administrative reads: root `Cargo.toml`, SDK `Cargo.toml`, clean `git status`, and
HEAD (`31e76ba8d93edc03cba1602df54b8f67dfd2fd1a`). The package is already a workspace member.
The frozen shared-crate baseline is not changed. No `AGENTS.md` was found in the worktree.
The initial `cargo doc --offline -p concord-sdk-mastodon --no-deps` succeeded without
`--locked`, using `CARGO_BUILD_JOBS=4` and the default worktree `target/`.

Public SDK documentation read before code: `README.md` and generated rustdoc for the crate,
`Mastodon`, `PublicTimelineRequest`, `StatusRequest`, `PublicStreamRequest`, `Status`, `Account`,
`ApiError`, `Error`, `InstanceInfo`, `StatusId`, `AccessToken`, `StreamingHost`, `PublicStream`,
`StreamEvent`, `StreamError`, `StreamErrorKind`, and `DiscoveryError`. Rustdoc Source links were
not followed.

Official documentation read on **2026-11-07** (research only, no API requests):

- [timelines API methods](https://docs.joinmastodon.org/methods/timelines/), public timeline
  request, bounds, response, and access rules.
- [streaming API methods](https://docs.joinmastodon.org/methods/streaming/), hostname discovery,
  HTTP SSE comments and event payloads, public stream authorization, and HTTP errors.

## Predictions

These predictions precede application code and any working SDK request.

### 1. Public timeline, then individual status

Construct `Mastodon::new(reqwest_client, InstanceUrl::parse(loopback_origin)?)`, optionally
`with_token(&AccessToken::new(synthetic_token)?)`. Call
`public_timeline().set_local(true).set_only_media(true).set_limit(TimelineLimit::new(2)?)`
then ordinary `.send().await`. It sends a bodyless `GET /api/v1/timelines/public` with
`local=true&only_media=true&limit=2`, `Accept: application/json`, and (when configured)
`Authorization: Bearer <synthetic_token>`. Unset `remote`, id bounds, and other flags are
absent. The output is `Vec<Status>`, with HTML content, opaque ids, author and visibility.
Call `status(page[0].id.clone()).send().await` next: bodyless `GET /api/v1/statuses/{id}`,
no query, same Accept and authorization, returning `Status`.

For older results, issue another timeline call with `set_max_id(last.id.clone())` and
the same filters. Paging is caller-controlled; `Link` does not drive requests. An empty
array returns an empty vector, with no automatic continuation and no proof the feed is
permanently exhausted (it may be disabled).

Non-2xx results produce `Error::Api`, retaining HTTP status and declared operation;
JSON `{"error": ...}` is available deliberately through `message()`/`document()`.
HTML/proxy or malformed error bodies keep the status without a provider document.
Malformed successful JSON or missing required status fields gives Request/Decode;
wrong response Content-Type gives Request/Metadata before decoding. Transport and size
failures remain Concord errors. Invalid limits (0, 41), ids and tokens fail locally.
Ordinary success returns the value without an HTTP response envelope; error metadata is
available through `api()`/`request()`. Tokens are sensitive native headers, absent from
queries, SDK Debug and error Display/Debug. Provider error text is explicit access only.

### 2. Discover host, then consume public SSE

Call `discover_streaming().await`: bodyless `GET /api/v2/instance`, no query,
`Accept: application/json` and the configured bearer header. It reads
`configuration.urls.streaming` and returns a `StreamingHost` bound to the REST instance.
The fixture will advertise a distinct loopback server using `ws://`; expect normalization
to `http://`. Then call `public_stream(&host)?.set_only_media(true).send().await`:
bodyless `GET /api/v1/streaming/public?only_media=true` on the discovered origin,
`Accept: text/event-stream`, same bearer header, no `access_token` or `Last-Event-ID`.
Opening returns a `PublicStream` after accepting the HTTP head.

`next().await` returns ordered `Update`, `StatusUpdate`, `Delete` or undecoded `Other`
events. Split UTF-8 and split `data:` fields are reassembled. Heartbeat comments such
as `:thump` produce no events but advance `position().bytes`. A malformed event payload
produces one nonterminal Payload error; subsequent events remain usable. Complete events
already received remain application-owned if a later event is partial or the connection
drops. A broken HTTP body produces one terminal Disconnected error with operation and
position; orderly EOF at a boundary is Closed, EOF inside an event is Truncated.
After the terminal error, `is_finished()` is true and further `next()` calls return None.
No reconnect, replay, deduplication, delivery guarantee, or resumption is implied.

Missing advertised configuration gives Discovery/NotAdvertised, never a guessed host;
missing required `configuration` is Request/Decode. Other-instance hosts and HTTPS-to-plain
advertisements fail locally. Opening with a missing/invalid token can return Api/401;
other non-2xx results retain status/provider errors. A successful HTTP head with the wrong
Content-Type is Request/Metadata. Stream diagnostics redact payload, token and URL;
unredacted sources require deliberate access.

## Friction log (ordered)

1. Generated rustdoc displays `impl ApiPublicTimeline`, `impl ApiStatus`, and
   `impl ApiPublicStream` under the public `*Request` pages. It also spells the stream
   result `Records<PublicStreamFormat, StreamEvent>` where the README calls it
   `PublicStream`. The documented examples use inferred types, so this does not yet
   block a caller. Potential shared-macro documentation improvement: consistently show
   the exported request and reader names. No shared files edited.
2. The `PublicStream::position()` return type (`Position`) was not linked in the SDK's
   `--no-deps` rustdoc. Looking for a generated `concord/stream/struct.Position.html`
   also found no page. The README's documented `.bytes` field was sufficient; no source
   lookup or explicit Position import was needed. This is a limitation of the available
   documentation set, not a demonstrated runtime defect.
3. The first caller test compiled and made its working call immediately, but a later
   test assertion incorrectly extended error URL-redaction to `Prepared` Debug. A
   backtrace localized the assertion to the prepared request: its Debug contains the
   loopback URL while the sensitive bearer value remains hidden. Corrected the caller
   assertion to check the token only; retained URL checks on errors. This was an
   overbroad caller expectation, not a demonstrated violation of the README's error
   contract. Two initial runs failed at this same assertion; no source lookup was used.

## First working call

`public_timeline().set_local(true).set_only_media(true).set_limit(TimelineLimit::new(2)?)`
followed by `.send().await` returned the two fixture statuses on the first compile/run of
`tests/first_use.rs`. No source lookup, extension trait, extra dependency, or SDK edit was
needed. The test then successfully retrieved the first status, fetched an empty older page,
classified provider/decode/metadata failures, and reached the later prepared-request
redaction assertion described in friction incident 3. Command:

`CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-mastodon --test first_use public_timeline_then_status_as_a_fresh_consumer -- --nocapture`

The tool output included `FIRST_WORKING_CALL: public_timeline().send() returned two statuses`.

## Prediction comparison and validation

### Workflow 1

| Prediction | Observation in the new caller test |
| --- | --- |
| Runtime instance, optional bearer, generated setters, ordinary `send()` | Matched. No extension trait, internal scope type, direct reqwest execution, or declaration change was necessary. |
| Timeline wire method/path/query/Accept, omitted unset parameters | Matched the exact target `/api/v1/timelines/public?local=true&only_media=true&limit=2`; body empty; bearer sent only as a header. |
| Timeline returns `Vec<Status>`; detail uses an id from that page | Matched: two realistic media-status documents; author, HTML, opaque id and public visibility asserted. Detail returned the edited content/timestamp for the first id, with no query. Media attachment members are intentionally outside the decoded slice. |
| Caller-controlled older paging; no automatic Link traversal | Matched: `max_id` used the last returned id. Exactly three requests covered the first page, detail and empty older page, despite an external Link target in the first response. The caller retained the original page and stopped; no permanent-exhaustion claim was made. |
| Access restrictions and provider/status errors | Matched: tokenless timeline 401, status 404 with an explicit error document, and proxy HTML 502 without a document. Status and `Api.*` operation metadata remained available. |
| Invalid JSON/required shape vs wrong media type | Matched: two Decode errors and a Metadata error. Each retained the successful 200 head and request id. Metadata retained an unread response; its fixture body stalled, proving the call did not wait to decode it. |
| Local validation and secrets | Matched: invalid limits, whitespace id and whitespace token failed without requests. Native authorization was sensitive, token absent from client/request/prepared Debug, and provider error text (deliberately echoing the synthetic token) was absent from error Display/Debug. Error diagnostics also omitted the origin. |

The only failed caller assertion was the extra assumption that **prepared-request Debug**
also omits the URL. It does not; the test now applies URL-redaction assertions to errors
and token-redaction assertions to prepared requests. This did not change a wire request
or the documented prediction that native prepared requests are inspectable.

### Workflow 2

| Prediction | Observation in the new caller test |
| --- | --- |
| Discover using instance configuration, not a guessed host | Matched: `GET /api/v2/instance`, JSON Accept, bearer, no query/body; a distinct loopback streaming origin advertised as `ws://` became `http://`. Discovery itself made no request to the streaming server. |
| Public SSE wire shape and credentials | Matched: `GET /api/v1/streaming/public?only_media=true`, event-stream Accept and bearer on the advertised origin; later opens omitted unset query parameters. No token query or Last-Event-ID header. All requests were bodyless GETs with the expected Host. |
| Open after head, then read incrementally | Matched: `position().bytes == 0` immediately after opening, before any `next()`. The ordinary request `.send()` inferred the documented reader without importing `Records` or `PublicStreamFormat`. |
| Chunk boundaries do not delimit SSE fields or characters | Matched: explicit HTTP chunks split `da` / `ta:` and the bytes of the sunflower character. The complete status and its Unicode author/content decoded correctly. A separate edited event used multiple SSE data lines and also decoded. |
| Comments/heartbeats are activity, not events; cancellation is safe | Matched: first returned item was the update after comments. On a second connection, a timeout cancelled pending `next()` after a heartbeat: bytes advanced by exactly the heartbeat length, no item was returned, and the later split-field delete still arrived. |
| Payload failure is nonterminal; unknown events remain text | Matched: one bad update yielded Payload with operation/codec/position metadata and redacted diagnostics. The next edited status, bare-id delete and unknown event followed in order; multiple unknown data lines became one string separated by a newline. |
| Disconnect preserves complete results, surfaces once, never replays | Matched: an unterminated HTTP chunked body after complete events and a partial update yielded Disconnected with a Transport cause and `Api.public_stream`. Byte position equalled the entire received prefix. The caller retained the completed status and invented none from the partial tail. `is_finished()` became true; repeated `next()` returned None; only one request had been issued at that point. |
| EOF and malformed UTF-8 are errors, not completion | Matched: a complete boundary EOF yielded Closed; an EOF inside a data line yielded Truncated; invalid UTF-8 yielded Malformed. Each was terminal and followed by None. Later connections in the test were separate, explicit caller opens, with no replay policy. |
| Discovery, access and response-metadata failures | Matched: missing advertised URL yielded NotAdvertised; missing required configuration yielded Decode with request-id metadata; discovery 503 kept its provider message. A wrong-type stream head yielded Metadata with its unread response/request id; tokenless opening yielded Api/401. |
| Origin binding and HTTPS downgrade guards | Matched with local public-value checks: another instance's host was refused before any request, and an HTTPS instance's plaintext advertisement was Insecure. |

No workflow behavior contradicted the Predictions section. The new caller tests do not
independently exercise every documented edge case: finite body/event size caps, REST transport
failure, malformed JSON **error** documents, redirects, unsupported discovery schemes, all id
bounds/filter combinations, and token lexical variants remain outside this small caller
application. The existing package suite was run separately; its source was not inspected.

### Commands and results

Every cargo invocation used `CARGO_BUILD_JOBS=4`, with `CARGO_TARGET_DIR` unset so builds
went into this worktree's default `target/`. Cargo commands were sequential. All commands
after initial unlocked documentation generation used `--offline --locked`.

- Generated SDK rustdoc first; then generated public Concord and testserver rustdoc using
  `cargo doc --offline --locked -p concord -p concord-example-testserver --no-deps`.
  Read the public Concord `Error`, `ErrorKind`, and `Operation` pages and public testserver
  crate, `TestServer`, `Recorded`, `Reply`, `Body`, `Script`, and `Gate` pages. No Source
  links followed. The attempt to list Concord docs before generation completed reported
  a missing output directory; subsequent generation completed successfully.
- Timeline caller's initial compile succeeded; two runs failed at the same overbroad
  prepared-Debug assertion. The corrected timeline caller passed (1 test).
- Adding the SSE caller compiled and passed immediately: **2 passed, 0 failed** in the
  focused `--test first_use` run.
- `rustfmt --edition 2024 examples/mastodon-sdk/tests/first_use.rs` completed.
- `cargo test --offline --locked -p concord-sdk-mastodon`: **47 passed, 0 failed**
  (2 fresh-consumer, 37 protocol, 4 existing workflow, 4 rustdoc tests; no unit tests).
- `cargo clippy --offline --locked -p concord-sdk-mastodon --all-targets -- -D warnings`:
  passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-mastodon --no-deps`:
  passed.

The lockfile is byte-for-byte identical to its pre-build snapshot. No new crates, package
versions, dependencies, manifest entries, or provenance changes were needed. Only
`FIRST_USE.md` and `tests/first_use.rs` were added. SDK source, shared crates, other examples,
checks and scripts were not edited; no commit was attempted. A shared-macro rustdoc naming
improvement is recorded above rather than implemented.

## Assistance label

**docs-only**. The SDK's `src/`, existing `tests/`, `FRICTION.md`, and `PROVENANCE.md` were
not inspected at any point. Existing tests were executed only after the first working
call. The new caller file is original consumer code based on public documentation;
compiler/test output and generated public rustdoc supplied all feedback. Live provider
calls: **NOT_RUN**.
