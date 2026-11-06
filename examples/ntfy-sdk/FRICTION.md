# Friction log — ntfy SDK (author)

Concord frozen at `6d6d66aa`; toolchain 1.99.1. Entries are ordered by how much SDK code they
cost or saved. Each has a reproduction (`file:line` at the commit that adds this file) and the
cause as I judge it: **Concord**, **docs**, **SDK**, **provider** or **application**.

Reading done: the root `README.md`, `docs/AUTHOR_GUIDE.md` (§4–§5.13), `docs/CUSTOM_FORMATS.md`,
`docs/CALLER_GUIDE.md` (§5–§6a), `docs/RECIPES.md`, the sources of `concord-core`
(`response.rs`, `codec.rs`, `error.rs`, `prepared.rs`) and `concord-extras` (`stream/`, `jsonl/`),
`checks/phase-a/tests/streams.rs`, and `examples/kubernetes-sdk` (`lib.rs`, `error.rs`,
`watch.rs`) — its `ApiError` (no message in `Display`, boxed document) shaped this crate's
`ApiError`. I deliberately did **not** read `examples/openai-sdk`, the first SSE consumer, so that
this SSE reader is an independent second one (decision W2-8).

## 1. Concord has no SSE reader: the framing is this SDK's own — Concord (W2-8 evidence)

- **What I had to write.** `src/sse.rs:42` (`Framer`: CR/LF/CRLF line ends split across chunks,
  a BOM split across chunks, comments, field splitting with one leading space removed, multi-line
  `data`, dispatch on an empty line, a bound on line + data, input ending inside an event) and
  `src/events.rs:173`/`:263` (`SseReader`: the chunk loop over `Incoming::chunk`, the unconsumed
  rest of a chunk, `Position`, `Completion`, fusing after an error, JSON decoding of `data` with a
  Format/Decode split). About 250 lines of library-shaped code plus 4 unit tests, before any ntfy
  meaning.
- **What ntfy needed from it.** Only `data` (one JSON object per event) and "an event started"
  (`event:` lines exist but the JSON `event` member is authoritative, because ntfy omits the SSE
  line for `message`, `message_delete` and `message_clear`). `id:` and `retry:` are ignored: ntfy
  sends neither, and its replay cursor is the JSON `id` sent back as `since=`.
- **What a shared reader would have to offer to serve this SDK unchanged:** frames of
  `(event type, data bytes)` (and `id`/`retry` for others) across arbitrary chunks; a configurable
  bound; "ended inside an event" as an error, not the browser's silent discard; the operation
  identity in errors; `Completion`/`Position`. ntfy puts all provider meaning in the JSON payload,
  so the frame layer is generic.
- **Reproduction:** `tests/protocol.rs:250` (every chunk boundary, one byte per chunk),
  `:273` (BOM, CRLF, lone CR, comments, unknown fields), `:303` (EOF inside an event), `:375`
  (bound), and the unit tests at the end of `src/sse.rs` (every two-way split of the input).

## 2. A reader of my own cannot use Concord's reader vocabulary — Concord

- `concord::stream::ReadError` has only `pub(crate)` constructors
  (`crates/concord-extras/src/stream/error.rs:187`), and `RecordReader` is sealed
  (`crates/concord-extras/src/stream/records.rs:15`, `:34`). So the SSE reader cannot yield a
  `ReadError`, and gets neither `RecordStream` (`into_stream()`, cancellation-safe
  `futures_core::Stream`) nor `StreamWatch`.
- **Workaround:** `EventsError` (`src/events.rs:364`, ~130 lines) re-creates `ReadError`'s
  surface — `kind()` (reusing the public `ReadErrorKind`), `operation()`, `position()` (reusing
  `Position`), `codec()` (reusing `CodecError` for the static label and `event`/`line`/`column`),
  `source_unredacted()`, and a content-free `Display` modelled on `ReadError`'s — and wraps the
  NDJSON reader's `ReadError` so both modes share one error type (`src/events.rs:401`).
  `Events` has `next()` but no `into_stream()`.
- `ReadErrorKind` is `#[non_exhaustive]`, so the hand-written `Display` needs a guessed `_` arm
  (`src/events.rs:472`).
- **Reproduction:** a throwaway test in this crate calling `ReadError::new(..)` and implementing
  `RecordReader` for a type of its own failed with `E0624: associated function new is private` and
  `E0277: the trait bound Mine: stream::records::sealed::Sealed is not satisfied` (observed, then
  removed). Tests that pin the reconstructed surface: `tests/protocol.rs:320`,
  `:349`, `:524`.

## 3. The `text/event-stream` head check is re-implemented — Concord

- The standard formats parse `Content-Type` with `mime` in `concord-extras`' private `media`
  module (`crates/concord-extras/src/media.rs:30` `single`, `:80` `is_utf8`, `:89`
  `check_charset`); the public `jsonl::MediaPolicy` applies only to the JSON readers.
- **Workaround:** `check_event_stream` (`src/events.rs:108`): one field line, essence compared
  case-insensitively, a `charset` parameter must be `utf-8` (quotes removed). It is cruder than the
  standard formats' (no RFC 9110 parameter grammar: a quoted `;` would be mis-split), and its
  refusal labels differ in wording from theirs.
- **Reproduction:** `tests/protocol.rs:404` (accepted and refused heads; the refused response
  stays unread).

## 4. A standard reader's limits cannot be set from an `Incremental` wrapper — Concord (minor)

- **Easy part:** wrapping Concord's NDJSON reader to add head metadata took one delegation per
  step: `NdjsonFormat` (`src/events.rs:83`) forwards `check_content_type`/`context` to `Ndjson`
  and opens with `<Ndjson as Incremental<Event>>::open((), body)` (`src/events.rs:100`), keeping
  the endpoint's identity in every `ReadError`.
- **Gap:** only the preset options are reachable that way (16 MiB per line). A configured
  `JsonLinesReader` opens an `Incoming` only through `open(..) -> Result<_, OpenError>`, which
  re-checks the head, and `Incremental::open` returns the records, not a `Result`, so the refusal
  would have nowhere to go; `open_checked` is `pub(crate)`
  (`crates/concord-extras/src/jsonl/lines.rs:449`). The SDK therefore fixed its per-event bound
  for both modes at the NDJSON default (`MAX_EVENT_BYTES`, `src/events.rs:41`) instead of a
  bound of its choosing, and documents it as not configurable.

## 5. `Incoming::chunk` made the SSE reader possible inside the boundary — positive; one docs gap

- `SseReader` reads only `self.body.chunk().await` (`src/events.rs:283`); a mid-body failure is
  Concord's `Transport` error naming `Ntfy.topic.subscribe_sse` with the URL removed
  (`tests/protocol.rs:320` checks no `127.0.0.1` in `Display`). Without D-W23-2 this would have
  needed `into_response()` and the URL-bearing reqwest error.
- The documented "a failure is not remembered" (`crates/concord-core/src/response.rs:346`) is
  handled by the reader fusing itself (`completion = Failed`).
- **Docs gap:** `Incoming::chunk` does not say whether dropping a pending call loses data.
  `Events::next` is documented cancel-safe on the assumption that it does not (reqwest's
  `Response::chunk`); `tests/protocol.rs:444` observes it on loopback HTTP/1.1 (a 50 ms timeout
  drops the pending read, the next call returns the whole event). That is an observation, not a
  guarantee from Concord's docs.

## 6. The send policy gave every operation the SDK's error — positive

- `send NtfyPolicy;` (`src/lib.rs:110`) and one generic impl (`src/error.rs:173`, ~15 lines) make
  every `send()` — the publish and the three stream endpoints — return `Result<_, NtfyError>`
  with ntfy's error document, read with `Received::bytes_limited` and `Json`'s body-only decoder
  (`src/error.rs:43`–`44`). A stream endpoint's 2xx still returns the unread `Events`, and a
  refused head stays Concord's `Metadata` error with the response unread. No wrapper per
  operation. Reproduction: `tests/protocol.rs:562`, `:609`.

## 7. Head metadata into the reader through `Incremental::Context` — positive

- `X-Messages-Truncated: 1` is read in `context` and carried as `type Context = bool` into
  `Events::truncated()` (`src/events.rs:59`, `:83`): exactly the documented use of the context.
  Reproduction: `tests/protocol.rs:496`.

## 8. A secret placed through a header template must implement `Display` — Concord/SDK trade-off

- `headers { AUTHORIZATION = "Bearer {token}"; }` would need `AccessToken: Display`, which makes
  `format!("{token}")` print the secret. Instead the client value is a sensitive `HeaderValue`
  placed directly (`src/lib.rs:107`), built by `AccessToken::header_value` (`src/model.rs:96`)
  and set by an inherent `Ntfy::with_token` written next to the declaration (`src/lib.rs:170`):
  adding inherent methods to a generated type works. Side effect: the generated
  `set_authorization(HeaderValue)` stays public (documented as the route for other schemes).
  Reproduction: `tests/protocol.rs:191`.

## 9. Small representation costs — Concord design, documented

- `Text` encodes only `String` (`crates/concord-extras/src/text.rs:20`), so the size-checked
  `MessageText` body needed its own 8-line encoder `PlainText` (`src/model.rs:177`).
- `ToHeaderValue` is closed (`crates/concord-core/src/header.rs:36`), so `Priority`, `Tags` and
  `Cache` are sent through templates (`src/lib.rs:126`–`128`), as AUTHOR_GUIDE §6 says; one line
  each.
- Two endpoints on one path differing by a static query (`subscribe_json` and `poll` on `json`,
  `query { "poll" = "1"; }`, `src/lib.rs:154`) needed nothing special.

## 10. Operation identity has two types — Concord (minor)

- `concord::Error::operation()` and `Incoming::operation()` return `Option<Operation>` (with the
  method); `ReadError::operation()` and `OpenError::operation()` return `Option<&'static str>`
  (`crates/concord-extras/src/stream/error.rs:70`, `:244`). To give both modes one
  `EventsError::operation()`, the SSE path keeps only the name (`src/events.rs:193`), so
  `EventsError`'s `Display` shows `Ntfy.topic.subscribe_sse:` while `NtfyError`'s shows
  `GET Ntfy.topic.subscribe_sse:`.

## 11. SDK-caused findings

- clippy `result_large_err` fired on the test's `Result<_, NtfyError>` (an `ApiError` with the
  document inline); callers would hit it too. The document is boxed (`src/error.rs:24`).
- A mutation run (BOM handling disabled) survived the first BOM fixture, whose first line after
  the BOM was a comment: a missed BOM only turned it into an ignored unknown field. The fixture now
  puts a `data:` line right after the BOM (`fixtures/sse_crlf_bom.txt`); the mutation is caught by
  `tests/protocol.rs:273`. Other mutations run and caught: lone CR + LF pairing, EOF inside an
  event accepted, cursor moved by `open`/`keepalive` ids, truncation flag ignored.

## 12. Observations without a Concord cause

- A publish with an empty body goes out with neither `content-length` nor chunking (reqwest's
  behaviour, observed at `tests/protocol.rs:120`); whether ntfy treats it as an empty message is
  not documented on the pages used — provider/transport, not checked against a server.
- A subscription must not run on a `reqwest::Client` with a total `timeout` (it covers the body):
  stated in the README (`README.md:24`) — application.
