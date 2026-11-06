# Integration record — ntfy SDK

## Between waves 3-4 (shared SSE reader)

Library refinement R1: `concord::sse` (feature `sse` of `concord-extras` and the facade), on top
of Concord `92775757`. Role: implementer, a Claude agent session spawned by the coordinator (the
session's system prompt names model `claude-opus-5-5`; effort is not visible to the session; the
coordinator records requested vs observed). Not an independent review: nobody has reviewed this
change yet.

**Verdict: adopted, with one workaround.** The SSE framer of `src/sse.rs` (wave 3, FRICTION
entry 1), the chunk loop over `Incoming::chunk` and the hand-written `text/event-stream` head
check (FRICTION entry 3) are gone. `SseFormat` now delegates its head check to `concord::sse::Sse`
and reads the body with `SseReader::read`, configured to the same 16 MiB bound. The SDK keeps
what is ntfy's: decoding each event's `data` (malformed JSON is `Format`, a well-formed value that
is not an ntfy message is `Decode`), `Events`, `EventsError`, `ReplayCursor` and the
`X-Messages-Truncated` context. No public signature changed; `subscribe_sse().send()` still
returns `Events`. The workaround — counting lines so that a payload error keeps its `line`
position — is described under [gaps](#what-the-reader-could-not-express).

### Why `read` over the body, not `-> stream` with `SseReader::open`

The reader's documentation suggests declaring `-> stream` and opening the `Incoming` with a
configured `SseReader` when the bound is not the default. For this SDK that would change the
public API and the tested behaviour:

- `subscribe_sse().send()` would return `Incoming` instead of `Events` (the send policy is generic
  over the output);
- a refused head would be an `OpenError` instead of Concord's `Metadata` error with the response
  unread (`tests/protocol.rs::the_sse_head_is_checked_before_the_body_is_read`,
  `tests/first_use.rs::sse_rejects_wrong_media_type_before_waiting_for_body`);
- a broken body would carry reqwest's error, not Concord's (`EventsError::transport`,
  `tests/protocol.rs::a_broken_connection_is_a_transport_error_naming_the_endpoint`).

Calling `SseReader::open` from `Incremental::open` instead keeps the API but re-checks a head the
format already accepted, with an `OpenError` branch that cannot happen (`open_checked`, which
`Sse` uses, is crate-private: FRICTION entry 4's situation). So the endpoint stays `-> stream
Event` with `response SseFormat;`, the format's `check_content_type` is the real (and only) head
check — Concord's — and `open` reads the unread body through `SseReader::read`, which checks
nothing, over a small byte source that keeps Concord's transport error (the adapter OpenAI also
has).

### Before / after

Before (`src/events.rs` at `92775757`; plus the 293-line `src/sse.rs`):

```rust,ignore
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_event_stream(head) // 38 lines: one field line, essence, `charset` parameters
    }
    fn open(truncated: bool, body: Incoming) -> Events {
        Events::sse(body, truncated) // Framer::new(MAX_EVENT_BYTES), pending chunk, position
    }

impl SseReader {
    async fn next(&mut self) -> Option<Result<Event, EventsError>> {
        if self.completion != Completion::Reading {
            return None;
        }
        loop {
            if !self.pending.is_empty() {
                let (used, event) = match self.framer.feed(&self.pending) { /* fault */ };
                self.position.bytes += used as u64;
                let _ = self.pending.split_to(used);
                match event {
                    Some(event) => return Some(self.decode(event)),
                    None => continue,
                }
            }
            match self.body.chunk().await {
                Ok(Some(chunk)) => self.pending = chunk,
                Ok(None) => { /* framer.finish(): inside an event is an error */ }
                Err(error) => { /* Completion::Failed, Detail::Transport(error) */ }
            }
        }
    }
    // fault(), decode(): about 45 more lines
}
```

After (`src/events.rs`):

```rust,ignore
    const ACCEPT: Option<&'static str> = <Sse as Incremental<concord::sse::Event>>::ACCEPT;
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Sse as Incremental<concord::sse::Event>>::check_content_type(head)
    }
    fn open(truncated: bool, body: Incoming) -> Events {
        Events { source: Source::Sse(Box::new(SseSource::new(body))), truncated }
    }

    fn new(body: Incoming) -> Self {
        let operation = body.operation().map(|op| op.name());
        let reader = SseReader::new(SseOptions::new().max_event_bytes(MAX_EVENT_BYTES));
        Self {
            // The head was checked by `SseFormat::check_content_type`: `read` checks nothing.
            events: Some(reader.read(Body { incoming: body, chunk: Bytes::new(), offset: 0,
                lines: LineEnds::default() })),
            operation,
            stopped: Position::default(),
        }
    }

    async fn next(&mut self) -> Option<Result<Event, EventsError>> {
        let events = self.events.as_mut()?;
        let event = match events.next().await? {
            Ok(event) => event,
            Err(error) => return Some(Err(EventsError::read(error, self.operation))),
        };
        // ... `serde_json::from_str::<Event>(event.data())`, Format/Decode, as before

impl ByteSource for Body {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        let next = self.incoming.chunk().await.map_err(|error| {
            SourceError::new("reading the response body failed").with_source(error)
        })?;
        // The line count (the workaround): see below.
        self.lines.add(&self.chunk);
        self.offset += self.chunk.len() as u64;
        self.chunk = next.clone().unwrap_or_default();
        Ok(next)
    }
}
```

`EventsError` keeps its surface; its `Detail` is now the reader's `ReadError` (NDJSON or SSE) or
this crate's payload `CodecError`. `transport()` finds Concord's error as the SSE reader error's
source (`downcast_ref::<concord::Error>()`); `source_unredacted()` still returns its reqwest
error. `Cargo.toml` adds the facade feature `sse`; `lib.rs` drops `mod sse`. `Source` boxes both
readers (the SSE one is about twice the NDJSON one's size: clippy `large_enum_variant`).

### Lines (non-blank, non-comment; `git diff -U0 92775757`)

| | before | after | removed | added |
|---|---|---|---|---|
| production code (`src/events.rs`, `src/sse.rs`, `src/lib.rs`) | 608 | 417 | 316 | 125 |
| unit tests (`#[cfg(test)]`, moved from `sse.rs` to `events.rs`) | 83 | 101 | 83 | 101 |
| `tests/protocol.rs` | 567 | 621 | 7 | 61 |
| `Cargo.toml` | 18 | 18 | 1 | 1 |

Net production code: −191 lines. Among the 125 added: `SseSource` 55 (most of it the payload
decoding that moved from the old `decode`) and `Body` with its `ByteSource` 17. The line count
(the workaround) is about 30 of the 125: `LineEnds` 15, `Body::line_at` 8, and about 8 lines
inside `Body` and `SseSource::next`. `README.md` (not counted) now describes Concord's reader in
"Limitations".

FRICTION entry 2 still applies in part: `EventsError` stays, because a payload that does not
decode is this SDK's failure and `ReadError` still has no public constructor; `Events` still has
no `into_stream()`. Every SSE reader failure is now Concord's `ReadError`, wrapped.

### Behaviour changes

Observed with a throwaway probe through the public API (a test file printing each error's
`Display`, `codec()`, `read_error()`, `transport()` and the reader's completion), run at
`92775757` and on this change, then removed:

| Input (`/sse`) | Before | After |
|---|---|---|
| `data: {}\n`, then EOF | `Format`, "the event stream ended inside an event" (line 2) | `Format`, "the input ends inside an event, before the blank line that dispatches it" (line 1) |
| `data: {}`, then EOF | `Format`, same label (line 1) | `Format`, "the input ends inside a line" (line 1) |
| `data: \xff"x"\n\n` | `Format`, "an event's data is not one JSON value" (event 1, line 2, column 1), serde error underneath | `Format`, "a line is not valid UTF-8" (line 1, column 7), no underlying error, 0 bytes consumed |
| `id: 7\n` or `retry: 10\n`, then EOF | clean end (`EndOfInput`) | `Format`, "the input ends inside an event, ..." |
| `\xEF\xBB` (a partial byte order mark), then EOF | clean end | `Format`, "the input ends inside a line" |
| an `event:` line of 16 MiB − 100 bytes, then 120 bytes of `data` | `Format` (the data is not JSON): the bound counted line + data | `Limit`: the event type counts toward the bound |
| one `data` line over 16 MiB | `Limit`, "an event is larger than the reader's bound" (line 1) | `Limit`, "an event exceeds max_event_bytes" (line 1, limit 16777216) |
| the body breaks after one event | `Transport`, `codec()` `None`; "...: reading the events failed (after 1 events, 70 bytes)" | `Transport`, `codec()` "reading the response body failed"; "...: reading the events failed: reading the response body failed (after 1 events, 70 bytes)", as `/json` already printed |
| malformed JSON, or a JSON value that is not a message | `Format`/`Decode` (event 1, line 2, column n) | unchanged |

| `Content-Type` of a 2xx `/sse` answer | Before | After |
|---|---|---|
| `text/event-stream; charset=us-ascii` | refused | accepted (a subset of UTF-8) |
| `text/event-stream; charset=utf-8; charset=utf-8` | accepted | refused: "declares charset more than once" |
| `text/event-stream; charset` (no `=`) | accepted | refused: "is not a media type" |
| `text/event-stream, text/html` | refused: "is not text/event-stream" | refused: "is not a media type" |
| none | refused: "the event stream response has no single Content-Type" | refused: "the response has no Content-Type" |
| `...; charset=iso-8859-1` | refused: "the event stream declares a charset other than utf-8" | refused: "the response declares a charset other than utf-8" |

Every refusal is still Concord's `Metadata` error with the response unread. From the code (not
probed):

- **Invalid UTF-8 anywhere** — also in a comment or an unknown field, which the old framer
  ignored — ends the reader; ntfy writes only JSON `data` lines and `event:` lines.
- **`read_error()`** now returns the SSE reader's error for every framing, UTF-8, bound and
  transport failure of `/sse` (it was `None` in SSE mode); that `ReadError` has no operation
  (`SseReader::read` takes none), while `EventsError::operation()` still names the endpoint.
- **A payload that does not decode drops the response at once**: the reader hands its body back
  (`into_source`) for the line count. Before, the connection stayed open until `Events` was
  dropped; after the reader's own errors it still does.
- **Work per poll is bounded** by the reader (64 KiB of input between two yields); the old loop
  framed a ready chunk of comments and blank lines without yielding.

Unchanged and still tested: event kinds from the JSON `event` member, the `Format`/`Decode` split
of `data` with its `event`/`line`/`column` positions and serde error, the replay cursor,
`X-Messages-Truncated`, the 16 MiB bound, the endpoint in every error, `position()` (an event whose
`data` does not decode is not counted as delivered: the reader counts it, the SDK subtracts it),
cancel safety, fusing and `completion()`.

### Tests

- `tests/protocol.rs::a_body_ending_inside_an_event_is_not_a_clean_end`: the expected label is the
  reader's "the input ends inside an event, before the blank line that dispatches it" (truncation:
  the reader names an end inside an event apart from an end inside a line). Kind and completion
  assertions unchanged.
- `tests/protocol.rs::malformed_or_foreign_data_fails_without_quoting_it`: each case now states its
  expected `(event, line)` and whether an underlying error is kept. Malformed JSON and a foreign
  shape are unchanged (`(Some(1), Some(2))`, kept). Invalid UTF-8 becomes `(None, Some(1))`, none
  kept: the reader refuses the line that holds the byte before any event is dispatched, and its
  error has no source. Kind `Format` and the no-content check unchanged.
- Unit tests: the four tests of `src/sse.rs` moved to `src/events.rs` (`mod tests`) and read with
  `SseReader::read(Chunks)` at the bounds they used; same inputs and assertions (every two-way
  split and one byte per chunk included), except the partial byte order mark: `Ok(no events)`
  becomes `Format` at line 1 (behaviour change above). `line` is now an `Option` (the codec's).
- New: `tests/protocol.rs::a_payload_error_names_its_line_across_chunks` (a payload error's line
  over one chunk, over gated chunks that split a CR LF and follow a lone CR, and one byte per
  chunk) and the unit test `line_ends_at_every_split`, for the workaround. A mutation that stops
  `Body` counting a used-up chunk fails the gated case (observed, then reverted).
- `tests/first_use.rs`, `tests/workflows.rs`, `tests/common/` and the doctests are unchanged.

### What the reader could not express

1. **The line that dispatched an event.** `Event` has no line and `SseEvents` exposes none (its
   parser counts lines; only `Debug` shows them). ntfy's payload errors name the stream line that
   ended the event (README "Errors", the assertions above), so `Body` counts line ends the way the
   reader splits lines (CR LF, LF, CR; a CR LF across chunks once) over each used-up chunk, and on
   a payload failure `SseSource` takes the reader apart (`into_source`) to count the current
   chunk up to the reader's byte position. About 30 lines and a second pass over every byte. A
   public `SseEvents::lines()` (or a line on `Event`) would remove all of it.
2. **Concord's transport error.** Opening `Incoming` (or reading `reqwest::Response`) wraps
   reqwest's error in the `SourceError`; keeping `concord::Error` for `EventsError::transport()`
   needs a `ByteSource` over `Incoming::chunk` — now in two SDKs (OpenAI's `Body`, this one). An
   `impl ByteSource for Incoming` that keeps Concord's error as the source would serve both.
3. **An operation name for `read`.** `SseReader::read` takes none, so the SDK carries the endpoint
   for its own errors, and the wrapped `ReadError` names no operation.
4. **A configured reader in an `Incremental` format** (see above): no public way to open an
   already-checked `Incoming` with non-default options; `read` over a byte source is the way out,
   at the cost of items 2 and 3.

### Verification (observed)

- `CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-ntfy`, five runs: each 5 unit +
  12 first_use + 22 protocol + 6 workflows + 5 doctests passed, 0 failed. At `92775757` the same
  command gave 4 + 12 + 21 + 6 + 5.
- `cargo clippy --locked -p concord-sdk-ntfy --all-targets -- -D warnings`: clean.
  `cargo fmt -p concord-sdk-ntfy -- --check`: clean. `cargo doc -p concord-sdk-ntfy --no-deps`: no
  warnings.
- Not run: the repository gate (`scripts/verify.sh`), Windows. Offline only; no ntfy server.
