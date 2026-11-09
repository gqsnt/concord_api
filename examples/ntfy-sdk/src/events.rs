//! Reading a subscription or a poll: the [`Events`] reader, its two wire formats, its error and
//! the replay cursor.
//!
//! # Two wire modes, one reader
//!
//! | Endpoint | Wire | Format | Framing |
//! |---|---|---|---|
//! | `subscribe_sse` (`GET /<topic>/sse`) | `text/event-stream` | [`SseFormat`] | Concord's SSE reader (`concord::sse::SseReader`), [`MAX_EVENT_BYTES`] per event |
//! | `subscribe_json`, `poll` (`GET /<topic>/json`) | `application/x-ndjson` | [`NdjsonFormat`] | Concord's NDJSON reader (`concord::jsonl::Ndjson`) |
//!
//! Both carry the same JSON message ([`Event`]) and yield the same [`Events`] after `.send().await?`. ntfy's envelope is
//! the JSON object: in SSE, the `data` payload is that object and its `event` member names the
//! kind (ntfy writes an SSE `event:` line only for some kinds, so the SSE event type is not used).
//! The SSE reader frames events; this crate decodes each `data` itself, so that malformed JSON
//! (`Format`) and a well-formed value that is not an ntfy message (`Decode`) stay distinct.
//!
//! # What a reader proves
//!
//! - [`Completion::EndOfInput`]: the server ended the body cleanly at an event boundary. For a
//!   **poll** this is the normal end: every message the server chose to return was read — which
//!   is what its cache still holds, minus `Cache: no` messages, minus what the replay size cap cut
//!   ([`Events::truncated`]). It is not proof of complete history. A **subscription** does not end
//!   by itself; a caller that stops reading leaves it [`Completion::Reading`].
//! - [`Completion::Failed`]: a terminal [`EventsError`] ended it (a lost connection, input ending
//!   inside an event, malformed or oversized input). Events already returned stay returned.
//! - Nothing is deduplicated: a reconnect with a timestamp cursor, or the server itself, can
//!   deliver a message again. Deduplicating by [`Event::id`] is the caller's decision.

use std::fmt;

use concord::bytes::Bytes;
use concord::jsonl::{JsonLinesRecords, Ndjson};
use concord::reqwest::header::HeaderName;
use concord::sse::{Sse, SseEvents, SseOptions, SseReader};
use concord::stream::{ByteSource, Completion, Position, ReadError, ReadErrorKind, SourceError};
use concord::{CodecError, Incoming, Incremental, ResponseHead};

use crate::model::{Event, EventId, EventKind, Since};

/// The most bytes one event may take: an SSE event being assembled (its `data` lines, event type
/// and unfinished line: the SSE reader's `max_event_bytes`), or one NDJSON line. The same 16 MiB
/// as Concord's NDJSON reader default, far above ntfy's documented limits (a 4,096-byte message,
/// a 1 KB title, 512 bytes of tags).
pub const MAX_EVENT_BYTES: usize = concord::jsonl::DEFAULT_MAX_RECORD_BYTES;

/// `X-Messages-Truncated`: a replay response that dropped older messages to fit the server's
/// per-topic replay size cap carries `1`.
pub const X_MESSAGES_TRUNCATED: HeaderName = HeaderName::from_static("x-messages-truncated");

fn truncated(head: &ResponseHead<'_>) -> bool {
    head.headers()
        .get_all(&X_MESSAGES_TRUNCATED)
        .iter()
        .any(|value| value.as_bytes() == b"1")
}

/// The format of `GET /<topic>/sse`: Concord's `text/event-stream` head check (exactly one
/// `Content-Type`, charset absent, `utf-8` or its subset `us-ascii`) before the body is read, then
/// [`Events`] over Concord's SSE reader with a [`MAX_EVENT_BYTES`] bound. It suggests
/// `Accept: text/event-stream`.
pub struct SseFormat(());

impl Incremental<Event> for SseFormat {
    type Records = Events;
    /// Whether the head said `X-Messages-Truncated: 1`.
    type Context = bool;
    const ACCEPT: Option<&'static str> = <Sse as Incremental<concord::sse::Event>>::ACCEPT;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Sse as Incremental<concord::sse::Event>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<bool, CodecError> {
        <Sse as Incremental<concord::sse::Event>>::context(head)?;
        Ok(truncated(head))
    }

    fn open(truncated: bool, body: Incoming) -> Events {
        Events {
            source: Source::Sse(Box::new(SseSource::new(body))),
            truncated,
        }
    }
}

/// The format of `GET /<topic>/json` (subscription and poll): Concord's NDJSON reader
/// (`application/x-ndjson` or `application/ndjson`, charset absent or UTF-8, blank lines skipped,
/// a line at most [`MAX_EVENT_BYTES`]), wrapped as [`Events`] with the head's truncation flag.
pub struct NdjsonFormat(());

impl Incremental<Event> for NdjsonFormat {
    type Records = Events;
    /// Whether the head said `X-Messages-Truncated: 1`.
    type Context = bool;
    const ACCEPT: Option<&'static str> = <Ndjson as Incremental<Event>>::ACCEPT;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Ndjson as Incremental<Event>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<bool, CodecError> {
        <Ndjson as Incremental<Event>>::context(head)?;
        Ok(truncated(head))
    }

    fn open(truncated: bool, body: Incoming) -> Events {
        Events {
            source: Source::Json(Box::new(<Ndjson as Incremental<Event>>::open((), body))),
            truncated,
        }
    }
}

/// The events of one subscription or poll response, read as the body arrives: what the generated
/// `send()` of `subscribe_sse`, `subscribe_json` and `poll` returns on success once the response head was
/// accepted (nothing of the body is read before the first [`next`](Self::next)).
///
/// - [`next`](Self::next) returns every event in order — `open`, `keepalive`, `message`, and kinds
///   the SDK does not know ([`EventKind::Other`]) — and `None` at the end of the body or after a
///   terminal error (the reader is then fused).
/// - It is **cancel-safe**: dropping a pending `next()` future (a `tokio::select!` branch, a
///   timeout) loses nothing; a partly received event stays buffered for the next call. Nothing
///   runs ahead of the caller and no task is spawned.
/// - Dropping the reader drops the response: the connection is closed, which ends the
///   subscription on the server.
///
/// `Debug` shows the mode, the position and the completion, never event content.
pub struct Events {
    source: Source,
    truncated: bool,
}

/// Both boxed: the readers are a few hundred bytes each, the SSE one about twice the NDJSON one
/// (clippy `large_enum_variant`).
enum Source {
    Sse(Box<SseSource>),
    Json(Box<JsonLinesRecords<Event>>),
}

/// The SSE mode: Concord's reader frames the body, this crate decodes each event's `data`.
struct SseSource {
    /// The reader; `None` once a `data` payload did not decode (its body was taken back to name
    /// the line, then dropped: the connection is closed).
    events: Option<SseEvents<Body>>,
    /// The endpoint, for this crate's payload errors (the reader names it in its own).
    operation: Option<&'static str>,
    /// Where reading stopped when a payload did not decode (that event not delivered).
    stopped: Position,
}

/// Which wire mode a reader parses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Server-Sent Events (`/sse`).
    Sse,
    /// Line-delimited JSON (`/json`).
    Json,
}

impl Events {
    /// The next event; `None` at the end of the body and after a terminal error.
    pub async fn next(&mut self) -> Option<Result<Event, EventsError>> {
        match &mut self.source {
            Source::Sse(sse) => sse.next().await,
            Source::Json(records) => records
                .next()
                .await
                .map(|item| item.map_err(|error| EventsError::read(error, None))),
        }
    }

    /// Whether the body ended cleanly ([`Completion::EndOfInput`]), a terminal error ended the
    /// reader ([`Completion::Failed`]), or events may follow ([`Completion::Reading`], also after
    /// the caller stopped reading).
    pub fn completion(&self) -> Completion {
        match &self.source {
            Source::Sse(sse) => sse
                .events
                .as_ref()
                .map_or(Completion::Failed, SseEvents::completion),
            Source::Json(records) => records.completion(),
        }
    }

    /// Events delivered and body bytes consumed so far.
    pub fn position(&self) -> Position {
        match &self.source {
            Source::Sse(sse) => sse.events.as_ref().map_or(sse.stopped, SseEvents::position),
            Source::Json(records) => records.position(),
        }
    }

    /// Whether the response said `X-Messages-Truncated: 1`: the server dropped older cached
    /// messages from this replay to fit its replay size cap. Read it before trusting a replay as
    /// the whole cache.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The wire mode.
    pub fn mode(&self) -> Mode {
        match self.source {
            Source::Sse(_) => Mode::Sse,
            Source::Json(_) => Mode::Json,
        }
    }
}

impl fmt::Debug for Events {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Events")
            .field("mode", &self.mode())
            .field("position", &self.position())
            .field("completion", &self.completion())
            .field("truncated", &self.truncated)
            .finish()
    }
}

impl SseSource {
    fn new(body: Incoming) -> Self {
        let operation = body.operation().map(|op| op.name());
        let reader = SseReader::new(SseOptions::new().max_event_bytes(MAX_EVENT_BYTES));
        let body = Body {
            incoming: body,
            chunk: Bytes::new(),
            offset: 0,
            lines: LineEnds::default(),
        };
        // The head was checked by `SseFormat::check_content_type`: `read_for` checks nothing.
        let events = match operation {
            Some(operation) => reader.read_for(body, operation),
            None => reader.read(body),
        };
        Self {
            events: Some(events),
            operation,
            stopped: Position::default(),
        }
    }

    /// Cancel-safe: the reader's `next` is, and nothing changes here until it returns.
    async fn next(&mut self) -> Option<Result<Event, EventsError>> {
        let events = self.events.as_mut()?;
        let event = match events.next().await? {
            Ok(event) => event,
            Err(error) => return Some(Err(EventsError::read(error, self.operation))),
        };
        let error = match serde_json::from_str::<Event>(event.data()) {
            Ok(decoded) => return Some(Ok(decoded)),
            Err(error) => error,
        };
        // Terminal. The reader counted the event as delivered; it is not.
        let mut stopped = events.position();
        stopped.records -= 1;
        self.stopped = stopped;
        let line = self
            .events
            .take()
            .map_or(0, |events| events.into_source().line_at(stopped.bytes));
        let (kind, label) = match error.classify() {
            serde_json::error::Category::Data => (
                ReadErrorKind::Decode,
                "an event's data is not an ntfy message",
            ),
            _ => (
                ReadErrorKind::Format,
                "an event's data is not one JSON value",
            ),
        };
        // The 1-based event, the stream line that ended it, the column in its data.
        let codec = CodecError::new(label)
            .at("event", stopped.records + 1)
            .at("line", line)
            .at("column", error.column() as u64)
            .with_source(error);
        Some(Err(EventsError::new(
            kind,
            self.operation,
            stopped,
            Detail::Payload(codec),
        )))
    }
}

/// The `/sse` body as the SSE reader's byte source: the `Incoming` (itself a byte source, whose
/// failure carries Concord's own transport error, naming the endpoint, URL removed, for
/// [`EventsError::transport`]), counting line ends for a payload error's `line`: the reader does
/// not say which line dispatched an event.
struct Body {
    incoming: Incoming,
    /// The chunk the reader is consuming (shared with it, not copied) and its offset in the body.
    chunk: Bytes,
    offset: u64,
    /// The line ends of the body before `chunk`.
    lines: LineEnds,
}

impl Body {
    /// The line ends in the first `bytes` bytes of the body (an offset within the current chunk):
    /// the number of the line that ends there.
    fn line_at(&self, bytes: u64) -> u64 {
        let used = usize::try_from(bytes.saturating_sub(self.offset)).unwrap_or(usize::MAX);
        let mut lines = self.lines;
        lines.add(&self.chunk[..used.min(self.chunk.len())]);
        lines.count
    }
}

impl ByteSource for Body {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        // Cancel-safe: nothing changes until `Incoming`'s returns, and it loses no byte.
        let next = self.incoming.next_chunk().await?;
        // The reader asks for a chunk only once it has used up the one it had.
        self.lines.add(&self.chunk);
        self.offset += self.chunk.len() as u64;
        self.chunk = next.clone().unwrap_or_default();
        Ok(next)
    }
}

/// Line ends as the event stream interpretation splits lines: CR LF, LF or CR, a CR LF split
/// across chunks counted once.
#[derive(Clone, Copy, Default)]
struct LineEnds {
    count: u64,
    after_cr: bool,
}

impl LineEnds {
    fn add(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\r' || (b == b'\n' && !self.after_cr) {
                self.count += 1;
            }
            self.after_cr = b == b'\r';
        }
    }
}

/// A failure after a reader started reading the body. Events returned before it stay returned;
/// the reader is fused (every later `next()` returns `None`).
///
/// | [`kind`](Self::kind) | Meaning |
/// |---|---|
/// | `Transport` | the connection broke mid-body (reconnect with a [`ReplayCursor`] to resume) |
/// | `Format` | input ended inside a line or an event, malformed JSON, invalid UTF-8 |
/// | `Decode` | a well-formed JSON value that is not an ntfy message |
/// | `Limit` | one event over [`MAX_EVENT_BYTES`] |
///
/// `Display` and `Debug` show the endpoint, the kind, a static label and positions — never event
/// content or the URL. The parser's or transport's own error is reachable deliberately through
/// [`source_unredacted`](Self::source_unredacted).
pub struct EventsError {
    inner: Box<Inner>,
}

struct Inner {
    kind: ReadErrorKind,
    operation: Option<&'static str>,
    position: Position,
    detail: Detail,
}

enum Detail {
    /// Concord's reader's error (NDJSON or SSE), as it returned it.
    Read(ReadError),
    /// An SSE event's `data` that is not one JSON value or not an ntfy message.
    Payload(CodecError),
}

impl EventsError {
    fn new(
        kind: ReadErrorKind,
        operation: Option<&'static str>,
        position: Position,
        detail: Detail,
    ) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                operation,
                position,
                detail,
            }),
        }
    }

    /// A reader's error; `operation` names the endpoint when the reader was not given it.
    fn read(error: ReadError, operation: Option<&'static str>) -> Self {
        Self::new(
            error.kind(),
            error.operation().or(operation),
            error.position(),
            Detail::Read(error),
        )
    }

    /// The stage of the failure.
    pub fn kind(&self) -> ReadErrorKind {
        self.inner.kind
    }

    /// The declared endpoint whose body failed (`Ntfy.topic.subscribe_sse`, …).
    pub fn operation(&self) -> Option<&'static str> {
        self.inner.operation
    }

    /// Events delivered and bytes consumed before the failure.
    pub fn position(&self) -> Position {
        self.inner.position
    }

    /// The static label and numeric positions (`line`, `column`) of the failure.
    pub fn codec(&self) -> Option<&CodecError> {
        match &self.inner.detail {
            Detail::Read(error) => error.codec(),
            Detail::Payload(codec) => Some(codec),
        }
    }

    /// Concord's reader's own error: the NDJSON reader's for a `/json` body, the SSE reader's for
    /// a `/sse` body (framing, UTF-8, the bound, the transport). `None` for an SSE event whose
    /// `data` does not decode (this crate's check).
    pub fn read_error(&self) -> Option<&ReadError> {
        match &self.inner.detail {
            Detail::Read(error) => Some(error),
            Detail::Payload(_) => None,
        }
    }

    /// Concord's `Transport` error, for a broken SSE body.
    pub fn transport(&self) -> Option<&concord::Error> {
        // The SSE body's source error is Concord's (`Incoming` read as the byte source); the
        // NDJSON reader's is reqwest's (`Ndjson` opens the native response).
        self.read_error()
            .filter(|error| error.kind() == ReadErrorKind::Transport)?
            .source_unredacted()?
            .downcast_ref::<concord::Error>()
    }

    /// The underlying error, **unredacted**: a serde error quoting the input, a transport error.
    pub fn source_unredacted(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        if let Some(error) = self.transport() {
            return error
                .transport()
                .map(|e| e as &(dyn std::error::Error + Send + Sync + 'static));
        }
        match &self.inner.detail {
            Detail::Read(error) => error.source_unredacted(),
            Detail::Payload(codec) => codec.source_unredacted(),
        }
    }
}

impl fmt::Display for EventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.inner.operation {
            write!(f, "{operation}: ")?;
        }
        f.write_str(match self.inner.kind {
            ReadErrorKind::Transport => "reading the events failed",
            ReadErrorKind::Format => "malformed events",
            ReadErrorKind::Decode => "an event does not decode",
            ReadErrorKind::Limit => "an event is too large",
            _ => "reading the events failed",
        })?;
        if let Some(codec) = self.codec() {
            write!(f, ": {codec}")?;
        }
        write!(
            f,
            " (after {} events, {} bytes)",
            self.inner.position.records, self.inner.position.bytes
        )
    }
}

impl fmt::Debug for EventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventsError")
            .field("kind", &self.inner.kind)
            .field("operation", &self.inner.operation)
            .field("position", &self.inner.position)
            .field("codec", &self.codec())
            .finish()
    }
}

/// No `source()`: the underlying error may quote content.
impl std::error::Error for EventsError {}

/// Where a subscriber resumes after a disconnect, as a `since=` value.
///
/// Feed it every event received ([`observe`](Self::observe)); [`since`](Self::since) then gives:
///
/// 1. the id of the last `message` received ([`Since::Id`]), sent to the server as the replay
///    cursor; replay may include messages already received;
/// 2. before any message, the time of the first `open` received ([`Since::Time`]): the cached
///    messages from the moment the first connection was established. Messages of that second
///    that were already delivered can come again;
/// 3. before any event, `None`: the caller chooses (`Since::All`, or a live-only subscription).
///
/// `open` and `keepalive` ids are made per connection and are never a cursor; `message_delete`,
/// `message_clear`, `poll_request` and unknown kinds do not move it either (only `message` is
/// documented as a cached message). The cursor does not deduplicate: a replay can include a
/// message again (a timestamp cursor, the server's own behaviour), and whether to skip it is the
/// caller's decision.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayCursor {
    last_message: Option<EventId>,
    first_open: Option<u64>,
}

impl ReplayCursor {
    /// A cursor that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// A cursor resuming after a message id the caller kept (a persisted position).
    pub fn after(id: EventId) -> Self {
        Self {
            last_message: Some(id),
            first_open: None,
        }
    }

    /// Takes one received event into account.
    pub fn observe(&mut self, event: &Event) {
        match event.kind {
            EventKind::Message => self.last_message = Some(event.id.clone()),
            EventKind::Open if self.first_open.is_none() => self.first_open = Some(event.time),
            _ => {}
        }
    }

    /// The `since=` value to reconnect with (see the type documentation).
    pub fn since(&self) -> Option<Since> {
        match (&self.last_message, self.first_open) {
            (Some(id), _) => Some(Since::Id(id.clone())),
            (None, Some(time)) => Some(Since::Time(time)),
            (None, None) => None,
        }
    }

    /// The id of the last message observed.
    pub fn last_message(&self) -> Option<&EventId> {
        self.last_message.as_ref()
    }
}

#[cfg(test)]
mod tests {
    //! The framing this crate relies on, read by Concord's SSE reader (the former `src/sse.rs`
    //! framer's tests: the same inputs and assertions, but for the partial byte order mark), and
    //! `Body`'s line count.

    use concord::stream::Chunks;

    use super::*;

    /// A framing failure: the kind and the line the reader names.
    #[derive(Debug)]
    struct Fault {
        kind: ReadErrorKind,
        line: Option<u64>,
    }

    /// Reads `chunks` with a `max` bound and returns every dispatched payload, or the first fault.
    async fn frame(chunks: &[&[u8]], max: usize) -> Result<Vec<Vec<u8>>, Fault> {
        let reader = SseReader::new(SseOptions::new().max_event_bytes(max));
        let mut events = reader.read(Chunks::new(
            chunks.iter().map(|chunk| Bytes::copy_from_slice(chunk)),
        ));
        let mut out = Vec::new();
        while let Some(event) = events.next().await {
            match event {
                Ok(event) => out.push(event.into_data().into_bytes()),
                Err(error) => {
                    return Err(Fault {
                        kind: error.kind(),
                        line: error.codec().and_then(|codec| codec.get("line")),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Every way of cutting `input` in two gives the same events as the whole input.
    async fn same_at_every_split(input: &[u8]) -> Vec<Vec<u8>> {
        let whole = frame(&[input], 1 << 20).await.expect("frames");
        for cut in 0..=input.len() {
            let (a, b) = input.split_at(cut);
            assert_eq!(
                frame(&[a, b], 1 << 20).await.expect("frames"),
                whole,
                "cut at {cut}"
            );
        }
        let bytes: Vec<&[u8]> = input.chunks(1).collect();
        assert_eq!(
            frame(&bytes, 1 << 20).await.expect("frames"),
            whole,
            "one byte per chunk"
        );
        whole
    }

    #[tokio::test]
    async fn line_ends_bom_comments_and_fields() {
        let input = b"\xEF\xBB\xBFevent: open\r\ndata: {\"a\":1}\r\n\r\n: comment\rdata:x\ndata\nid: 7\nretry: 10\nunknown: y\n\nevent: keepalive\n\n";
        let events = same_at_every_split(input).await;
        assert_eq!(events, vec![b"{\"a\":1}".to_vec(), b"x\n".to_vec()]);
    }

    #[tokio::test]
    async fn empty_data_is_an_event_and_fieldless_blank_lines_are_not() {
        assert_eq!(
            same_at_every_split(b"\n\n\ndata:\n\n").await,
            vec![Vec::<u8>::new()]
        );
    }

    #[tokio::test]
    async fn end_inside_an_event_is_an_error() {
        for input in [
            &b"data: {}"[..],
            b"data: {}\n",
            b"event: open\n",
            b"data: {}\r",
        ] {
            // `data: {}\r` ends a line with a lone CR, but not the event.
            let fault = frame(&[input], 1 << 20).await.unwrap_err();
            assert_eq!(fault.kind, ReadErrorKind::Format, "{input:?}");
        }
        // Concord's reader: a partial byte order mark is content, so this body ends inside a line
        // (the former framer read it as an empty body).
        let fault = frame(&[b"\xEF\xBB"], 1 << 20).await.unwrap_err();
        assert_eq!((fault.kind, fault.line), (ReadErrorKind::Format, Some(1)));
        assert_eq!(frame(&[b""], 1 << 20).await.unwrap(), Vec::<Vec<u8>>::new());
    }

    #[tokio::test]
    async fn the_bound_counts_the_line_and_the_data() {
        // The raw line (`data: 12345`, 11 bytes) and the retained data count together.
        assert_eq!(
            frame(&[b"data: 12345\n\n"], 11).await.unwrap(),
            vec![b"12345".to_vec()]
        );
        assert_eq!(
            frame(&[b"data: 12345\n\n"], 10).await.unwrap_err().kind,
            ReadErrorKind::Limit
        );
        let fault = frame(&[b"data: 1234\ndata: 5\n\n"], 11).await.unwrap_err();
        assert_eq!((fault.kind, fault.line), (ReadErrorKind::Limit, Some(2)));
        let fault = frame(&[b"data: 1234567"], 8).await.unwrap_err();
        assert_eq!(fault.kind, ReadErrorKind::Limit);
    }

    /// `Body` counts line ends as the reader splits lines, wherever a chunk is cut.
    #[test]
    fn line_ends_at_every_split() {
        let input = b"a\r\nb\rc\n\r\n\r";
        for cut in 0..=input.len() {
            let (a, b) = input.split_at(cut);
            let mut lines = LineEnds::default();
            lines.add(a);
            lines.add(b);
            assert_eq!(lines.count, 5, "cut at {cut}");
        }
    }
}
