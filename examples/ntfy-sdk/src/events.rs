//! Reading a subscription or a poll: the [`Events`] reader, its two wire formats, its error and
//! the replay cursor.
//!
//! # Two wire modes, one reader
//!
//! | Endpoint | Wire | Format | Framing |
//! |---|---|---|---|
//! | `subscribe_sse` (`GET /<topic>/sse`) | `text/event-stream` | [`SseFormat`] | this crate's SSE framer over [`Incoming::chunk`] |
//! | `subscribe_json`, `poll` (`GET /<topic>/json`) | `application/x-ndjson` | [`NdjsonFormat`] | Concord's NDJSON reader (`concord::jsonl::Ndjson`) |
//!
//! Both carry the same JSON message ([`Event`]) and return the same [`Events`]. ntfy's envelope is
//! the JSON object: in SSE, the `data` payload is that object and its `event` member names the
//! kind (ntfy writes an SSE `event:` line only for some kinds, so the SSE event type is not used).
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
use concord::stream::{Completion, Position, ReadError, ReadErrorKind};
use concord::{CodecError, ContentTypeField, Incoming, Incremental, ResponseHead};

use crate::model::{Event, EventId, EventKind, Since};
use crate::sse::{Dispatched, Fault, Framer};

/// The most bytes one event may take: an SSE event's lines and data together, or one NDJSON line.
/// The same 16 MiB as Concord's NDJSON reader default, far above ntfy's documented limits (a
/// 4,096-byte message, a 1 KB title, 512 bytes of tags).
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

/// The format of `GET /<topic>/sse`: a `text/event-stream` response (charset absent or UTF-8),
/// checked before the body is read, opened as [`Events`] over this crate's SSE framer. It suggests
/// `Accept: text/event-stream`.
pub struct SseFormat(());

impl Incremental<Event> for SseFormat {
    type Records = Events;
    /// Whether the head said `X-Messages-Truncated: 1`.
    type Context = bool;
    const ACCEPT: Option<&'static str> = Some("text/event-stream");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_event_stream(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<bool, CodecError> {
        Ok(truncated(head))
    }

    fn open(truncated: bool, body: Incoming) -> Events {
        Events::sse(body, truncated)
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
            source: Source::Json(<Ndjson as Incremental<Event>>::open((), body)),
            truncated,
        }
    }
}

/// `text/event-stream`, compared case-insensitively on type and subtype; a `charset` parameter, if
/// present, must be `utf-8` (the stream is UTF-8 by definition). Exactly one field line.
fn check_event_stream(head: &ResponseHead<'_>) -> Result<(), CodecError> {
    let ContentTypeField::One(value) = head.content_type() else {
        return Err(CodecError::new(
            "the event stream response has no single Content-Type",
        ));
    };
    let Ok(text) = value.to_str() else {
        return Err(CodecError::new(
            "the response Content-Type is not visible ASCII",
        ));
    };
    let mut parts = text.split(';');
    let essence = parts.next().unwrap_or_default().trim_matches([' ', '\t']);
    if !essence.eq_ignore_ascii_case("text/event-stream") {
        return Err(CodecError::new(
            "the response Content-Type is not text/event-stream",
        ));
    }
    for parameter in parts {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        let value = value.trim_matches([' ', '\t']);
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        if name
            .trim_matches([' ', '\t'])
            .eq_ignore_ascii_case("charset")
            && !value.eq_ignore_ascii_case("utf-8")
        {
            return Err(CodecError::new(
                "the event stream declares a charset other than utf-8",
            ));
        }
    }
    Ok(())
}

/// The events of one subscription or poll response, read as the body arrives: what the generated
/// `send()` of `subscribe_sse`, `subscribe_json` and `poll` returns once the response head was
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

enum Source {
    Sse(SseReader),
    Json(JsonLinesRecords<Event>),
}

/// The SSE reader: the unread body, the framer, and the unconsumed rest of the current chunk.
struct SseReader {
    body: Incoming,
    operation: Option<&'static str>,
    framer: Framer,
    pending: Bytes,
    position: Position,
    completion: Completion,
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
    fn sse(body: Incoming, truncated: bool) -> Self {
        let operation = body.operation().map(|op| op.name());
        Self {
            source: Source::Sse(SseReader {
                body,
                operation,
                framer: Framer::new(MAX_EVENT_BYTES),
                pending: Bytes::new(),
                position: Position::default(),
                completion: Completion::Reading,
            }),
            truncated,
        }
    }

    /// The next event; `None` at the end of the body and after a terminal error.
    pub async fn next(&mut self) -> Option<Result<Event, EventsError>> {
        match &mut self.source {
            Source::Sse(reader) => reader.next().await,
            Source::Json(records) => records
                .next()
                .await
                .map(|item| item.map_err(EventsError::from_read)),
        }
    }

    /// Whether the body ended cleanly ([`Completion::EndOfInput`]), a terminal error ended the
    /// reader ([`Completion::Failed`]), or events may follow ([`Completion::Reading`], also after
    /// the caller stopped reading).
    pub fn completion(&self) -> Completion {
        match &self.source {
            Source::Sse(reader) => reader.completion,
            Source::Json(records) => records.completion(),
        }
    }

    /// Events delivered and body bytes consumed so far.
    pub fn position(&self) -> Position {
        match &self.source {
            Source::Sse(reader) => reader.position,
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

impl SseReader {
    async fn next(&mut self) -> Option<Result<Event, EventsError>> {
        if self.completion != Completion::Reading {
            return None;
        }
        loop {
            if !self.pending.is_empty() {
                let framed = self.framer.feed(&self.pending);
                let (used, event) = match framed {
                    Ok(step) => step,
                    Err(fault) => return Some(Err(self.fault(fault))),
                };
                self.position.bytes += used as u64;
                let _ = self.pending.split_to(used);
                match event {
                    Some(event) => return Some(self.decode(event)),
                    None => continue,
                }
            }
            // The only await: until it completes, no state has changed (cancel-safe).
            match self.body.chunk().await {
                Ok(Some(chunk)) => self.pending = chunk,
                Ok(None) => {
                    if let Err(fault) = self.framer.finish() {
                        return Some(Err(self.fault(fault)));
                    }
                    self.completion = Completion::EndOfInput;
                    return None;
                }
                Err(error) => {
                    self.completion = Completion::Failed;
                    return Some(Err(EventsError::new(
                        ReadErrorKind::Transport,
                        self.operation,
                        self.position,
                        Detail::Transport(error),
                    )));
                }
            }
        }
    }

    fn fault(&mut self, fault: Fault) -> EventsError {
        self.completion = Completion::Failed;
        let codec = CodecError::new(fault.label).at("line", fault.line);
        EventsError::new(
            fault.kind,
            self.operation,
            self.position,
            Detail::Sse(codec),
        )
    }

    fn decode(&mut self, event: Dispatched) -> Result<Event, EventsError> {
        match serde_json::from_slice::<Event>(&event.data) {
            Ok(decoded) => {
                self.position.records += 1;
                Ok(decoded)
            }
            Err(error) => {
                self.completion = Completion::Failed;
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
                    .at("event", self.position.records + 1)
                    .at("line", event.line)
                    .at("column", error.column() as u64)
                    .with_source(error);
                Err(EventsError::new(
                    kind,
                    self.operation,
                    self.position,
                    Detail::Sse(codec),
                ))
            }
        }
    }
}

/// A failure after a reader started reading the body. Events returned before it stay returned;
/// the reader is fused (every later `next()` returns `None`).
///
/// | [`kind`](Self::kind) | Meaning |
/// |---|---|
/// | `Transport` | the connection broke mid-body (reconnect with a [`ReplayCursor`] to resume) |
/// | `Format` | input ended inside an event, malformed SSE or JSON, invalid UTF-8 |
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
    /// The NDJSON reader's error, as it returned it.
    Read(ReadError),
    /// `Incoming::chunk` failed (Concord's `Transport` error, URL removed).
    Transport(concord::Error),
    /// The SSE framer's or payload decoder's failure.
    Sse(CodecError),
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

    fn from_read(error: ReadError) -> Self {
        Self::new(
            error.kind(),
            error.operation(),
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
            Detail::Transport(_) => None,
            Detail::Sse(codec) => Some(codec),
        }
    }

    /// The JSON stream reader's own error, for a `/json` body.
    pub fn read_error(&self) -> Option<&ReadError> {
        match &self.inner.detail {
            Detail::Read(error) => Some(error),
            _ => None,
        }
    }

    /// Concord's `Transport` error, for a broken SSE body.
    pub fn transport(&self) -> Option<&concord::Error> {
        match &self.inner.detail {
            Detail::Transport(error) => Some(error),
            _ => None,
        }
    }

    /// The underlying error, **unredacted**: a serde error quoting the input, a transport error.
    pub fn source_unredacted(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        match &self.inner.detail {
            Detail::Read(error) => error.source_unredacted(),
            Detail::Transport(error) => error
                .transport()
                .map(|e| e as &(dyn std::error::Error + Send + Sync + 'static)),
            Detail::Sse(codec) => codec.source_unredacted(),
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
