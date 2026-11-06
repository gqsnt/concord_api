# Independent first use

## Predictions recorded before implementation

### A. Subscribe to SSE through cancellation

Entry point: README, “Workflow A”. Construct `Ntfy` with a caller-owned HTTP client,
set the loopback base URL and a visibly synthetic `AccessToken`, then select a validated
`Topic`. `subscribe_sse().send()` should send `GET /first-use/sse` with bearer auth and
return `Events` after accepting `text/event-stream`, without waiting for EOF. Each
`next()` should yield `Some(Ok(Event))` for open, message and keepalive frames. The
JSON payload determines the kind; a message needs no SSE `event:` field.

Prediction: cancelling a pending `next()` in `tokio::select!` leaves completion at
`Reading` and preserves a partial event for a later `next()`. Dropping `Events` closes
the subscription. `ReplayCursor` starts empty, uses the first open time until a
message arrives, then uses the last message ID; keepalive IDs must not replace it.
A clean premature closure yields `None` and `EndOfInput`, requiring an explicit,
bounded application reconnect with `since=<last message id>`. A cut-off HTTP body,
unfinished SSE frame or bad event yields a terminal typed read error and `Failed`,
while already delivered messages and the cursor survive. A wrong successful media
type fails `send()` as `NtfyError::Request` / `Metadata`; a provider rejection is
`NtfyError::Api` with status and error document.

### B. Publish while the subscriber is offline, then replay JSON

Entry point: README, “Workflow B”. “Offline” means no subscriber is connected;
the publisher can still reach the provider. Publish text with
`publish(MessageText).set_title(...).set_priority(...).set_tags(...).send()`.
Prediction: `POST /first-use`, exact UTF-8 body, documented `X-*` headers and auth;
the returned `Event` includes the stored ID, message metadata and expiry. A publish
with `Cache::No` sends `X-Cache: no`, has no expiry and is absent from later replay.

Then `poll().set_since(Since::Id(last_seen)).send()` should request
`GET /first-use/json` with `poll=1` and `since=<id>` (query order immaterial), returning
an incremental NDJSON `Events` reader. It should yield only selected cached messages,
then `None` / `EndOfInput`, including an empty replay. The caller owns deduplication
and persists a cursor only for received messages. `X-Messages-Truncated: 1` must be
visible even after clean EOF: EOF is not complete history. A malformed or interrupted
replay must preserve its prefix, emit a typed error and finish `Failed`, never success.
HTTP 429 with an ntfy error document should fail before a reader is returned and retain
the provider code and delta-seconds `Retry-After`, without automatic retry.

## Ordered action and evidence trace

1. Started with the SDK README; inspected its manifest and `.gitattributes`. Working
   tree was clean. No SDK source, existing SDK tests, author notes or `FRICTION.md`
   were read. No existing provider SDK supplied this solution.
2. Read Concord's root README, caller guide, relevant streaming/diagnostic recipe
   references, extras/pagination READMEs, and generated public rustdoc. Both requested
   doc builds succeeded on pinned Rust 1.99.1. Cargo briefly waited for the shared
   build-directory lock, then completed without intervention. The SSE/JSON poll API
   uses streaming, not page traversal.
3. Consulted the official [subscription documentation](https://docs.ntfy.sh/subscribe/api/)
   (SSE framing, JSON lines, poll, since and replay limits) and
   [publish documentation](https://docs.ntfy.sh/publish/) (text body, priority, tags,
   bearer auth and cache opt-out). Read `examples/testserver/src/lib.rs` to learn the
   fake server's public `Reply`, `Body`, recorded-request and cleanup facilities;
   this is test infrastructure, not an SDK source intervention.
4. Wrote the predictions above before creating the test implementation. The fake and
   new `fixtures/first-use/` bytes are independently hand-authored from those protocol
   descriptions, with synthetic IDs, times, text and provider errors. They are not
   copied vendor captures or live conformance evidence. Existing fixture files were
   not reused. Existing `fixtures/** -text` preserves the new bytes too.
5. Implemented A using only documented SDK entry points and the loopback server.
   The first `cargo test --locked -p concord-sdk-ntfy --test first_use` compiled
   without warnings or compiler errors and passed all five SSE tests. Observed:
   pending-read cancellation preserved a split UTF-8 payload, dropping the live
   reader disconnected the server writer, a clean close allowed one explicit
   reconnect with the message ID, and format/decode/transport failures retained
   one delivered event and stayed `Failed`. Wrong media type returned before a
   stalled body; a 403 retained its typed document without retrying.
6. Began B after that result, using the already-recorded B prediction. The new fake
   derives cached JSON events from recorded publish requests; it does not use SDK
   types to parse its requests or generate its responses. Cache selection and
   timestamps are explicit test choices, not claims about a real server's ordering.
7. The expanded suite compiled without warnings or compiler errors: ten tests passed,
   one failed. In `failed_json_replays_retain_prefix_and_never_report_success`, the
   “complete JSON value but no final newline” case expected `Format`; `unwrap_err()`
   instead received `Ok(Event { id: EventId("replayed01"), ... })`. This contradicts
   the initial reading of README's “body that ends inside ... a JSON line (`Format`)”.
   No source read was necessary. Kept the original prediction above; changed the
   actual truncated-JSON failure fixture to omit its closing brace, and added an
   explicit public-API regression showing newline-less valid JSON reaches clean EOF.
8. The corrected suite passed all twelve tests. Consulted Concord's public
   `concord::jsonl` rustdoc framing table: it explicitly accepts a final line without
   LF at true EOF. The observed behavior therefore matches Concord's documented
   policy; the SDK README's completion wording caused the failed prediction.
9. Reviewed the new fake's cancellation gate and made it emit no bytes while the
   payload is paused, so waiting cannot insert heartbeat text inside partial JSON.
   Added a byte-position assertion proving the pending read consumed the partial
   frame. Formatted only the new test file with `rustfmt --edition 2024` and reran
   the requested command: **12 passed, 0 failed**, without compiler warnings or
   errors. Live and partial-stream waits have deadlines; test-server shutdown owns and closes
   remaining sockets. `git check-attr text` reported `unset` for every new fixture.

## Working paths and validation

- A: `Ntfy::new` → `with_base_url` / `with_token` → `topic` →
  `subscribe_sse().maybe_since(cursor.since()).send()` → `Events::next()` with
  `tokio::select!` → observe cursor / inspect completion → drop the reader.
  Verified request method, path, auth, absence/presence of the cursor; event kinds,
  UTF-8 text and cursor progression; cancel-safe partial framing; bounded reconnect;
  clean EOF versus terminal format/decode/transport errors; metadata rejection and
  a provider 403 document.
- B: publish while no subscriber exists → persist the returned ID →
  `poll().set_since(Since::Id(id)).send()` → consume JSON events / deduplicate →
  require clean completion and inspect truncation → persist the new cursor.
  Verified exact publish bytes and headers, returned metadata, cache opt-out,
  cursor selection, an empty follow-up poll, repeated timestamp results, fragmented
  NDJSON with blank lines, truncated-cache metadata, partial failures and a 42905
  document with `Retry-After`. Publish rejection also retains the typed provider error.

Final command (also used for each test attempt):

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-ntfy --test first_use
```

Fixture provenance: `live.sse` and `replay.jsonl` are hand-authored synthetic messages
using the official stream shapes; `publish.txt` is synthetic multiline UTF-8 input.
`forbidden.json` uses the SDK README/rustdoc's documented error shape and code;
`bandwidth.json` uses that shape with the official subscription page's `42905` code.
The `malformed.*` and `missing-fields.*` files are intentionally invalid synthetic
responses. Newline-less, incomplete and transport-cut-off bodies are produced by
slicing those fixture bytes or the test server's documented failure body. Stateful
cache responses are generated from recorded wire requests using `serde_json`, not
SDK serialization. No preexisting SDK fixture, source or test was read or reused.

## Interventions and friction

- **First intervention: none.** No SDK source or existing SDK tests were read, and
  no native escape, private reach-through, rebuilt execution or handwritten
  operation identity was needed. There were no compiler diagnostics. The only
  failing test was the documented newline expectation above. SDK source, README,
  existing tests, frozen Concord files and other examples were left unchanged.
- **SDK documentation ambiguity: final JSON line.** Reproduce with
  `json_poll_accepts_complete_final_value_without_newline_despite_readme_prediction`:
  return the first `replay.jsonl` value without its LF as `application/x-ndjson`.
  It yields the event and `EndOfInput`; only cutting into the JSON value yields
  `Format` / `Failed`. The original README-based expectation failed. The working
  test records the actual behavior; no library workaround is required.
- **Completion requires two signals (documented caller obligation).** After a
  terminal read error, later `next()` calls return `None` just as they do after clean
  EOF. Reproduce with `failed_json_replays_retain_prefix_and_never_report_success`:
  one cached event is delivered, the next read errors, subsequent reads return
  `None`, and `completion()` stays `Failed`. Likewise the truncated-poll test ends
  cleanly with `truncated() == true`. A consumer must check both completion and cache
  truncation before treating replay as successful catch-up; EOF is not history.
- **Reconnect and duplicate policy remain application work (documented).** The
  bounded-reconnect test makes two explicit subscription requests after clean
  server closes, resuming with `since=message001`. The timestamp-replay test gets
  two events and uses a `HashSet<EventId>` to deliver only one unseen message.
  These are visible application responsibilities, not hidden SDK retries or
  evidence that a shared-library change is necessary.
