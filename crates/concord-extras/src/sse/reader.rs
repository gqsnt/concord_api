//! The options, the reader, its events and the response head check.

use std::fmt;
use std::time::Duration;

use concord_core::{CodecError, ResponseHead, reqwest};

use super::parse::{Parser, Step};
use crate::media;
use crate::stream::{
    ByteSource, Completion, Driver, OpenError, Openable, Position, ReadError, RecordStream,
    SKIP_BUDGET, open_input, yield_now,
};

/// The default [`SseOptions::max_event_bytes`]: 1 MiB.
pub const DEFAULT_MAX_EVENT_BYTES: usize = 1024 * 1024;

/// The configuration of an [`SseReader`].
///
/// | | default |
/// |---|---|
/// | [`max_event_bytes`](Self::max_event_bytes) | 1 MiB ([`DEFAULT_MAX_EVENT_BYTES`]) |
/// | [`max_events`](Self::max_events) | none |
#[derive(Clone, Debug)]
pub struct SseOptions {
    pub(super) max_event_bytes: usize,
    pub(super) max_events: Option<u64>,
}

impl SseOptions {
    /// The defaults (see the type documentation).
    pub fn new() -> Self {
        Self {
            max_event_bytes: DEFAULT_MAX_EVENT_BYTES,
            max_events: None,
        }
    }

    /// The largest event being assembled, in bytes: its data buffer (every `data` value, each
    /// followed by a line feed), its event type and the line still being received (field name
    /// included, line end excluded; a comment line too while it is being received). Complete
    /// comments, blank lines, `id` and `retry` values and dispatched events do not count. An
    /// event that would need more is a terminal [`Limit`](crate::stream::ReadErrorKind::Limit)
    /// error, detected before a buffer grows past this size; it is never truncated.
    #[must_use]
    pub fn max_event_bytes(mut self, max: usize) -> Self {
        self.max_event_bytes = max;
        self
    }

    /// The most events to deliver. A further event is a terminal
    /// [`Limit`](crate::stream::ReadErrorKind::Limit) error, never a clean end; an input with
    /// exactly this many events ends cleanly.
    #[must_use]
    pub fn max_events(mut self, max: u64) -> Self {
        self.max_events = Some(max);
        self
    }
}

impl Default for SseOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// One dispatched server-sent event.
///
/// `Debug` shows the content: an event is the caller's data. The reader's errors never do.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Event {
    event: String,
    data: String,
    id: String,
}

impl Event {
    pub(super) fn new(event: String, data: String, id: String) -> Self {
        Self { event, data, id }
    }

    /// The event type: the last `event` field of the event, `message` when it had none (or an
    /// empty one).
    pub fn event(&self) -> &str {
        &self.event
    }

    /// The data: the event's `data` values joined with a line feed (`\n`). Not decoded: parse it
    /// yourself (`serde_json::from_str(event.data())`).
    pub fn data(&self) -> &str {
        &self.data
    }

    /// The data, taken.
    pub fn into_data(self) -> String {
        self.data
    }

    /// The last event ID when the event was dispatched: the value of the most recent valid `id`
    /// field of the stream so far (this event's or an earlier one's), empty if none.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// An incremental server-sent events reader: an immutable configuration that opens responses
/// (or reads any [`ByteSource`]) as [`SseEvents`]. See the [module](super) for the rules.
///
/// ```no_run
/// # async fn demo(response: concord_core::reqwest::Response) -> Result<(), Box<dyn std::error::Error>> {
/// use concord_extras::sse::{SseOptions, SseReader};
///
/// let reader = SseReader::new(SseOptions::new().max_event_bytes(64 * 1024));
/// let mut events = reader.open(response)?;
/// while let Some(event) = events.next().await {
///     let event = event?;
///     if event.event() == "done" {
///         break;
///     }
///     let value: serde_json::Value = serde_json::from_str(event.data())?;
///     # let _ = value;
/// }
/// # Ok(()) }
/// ```
#[derive(Clone, Debug, Default)]
pub struct SseReader {
    options: SseOptions,
}

impl SseReader {
    /// A reader with these options.
    pub fn new(options: SseOptions) -> Self {
        Self { options }
    }

    /// The options.
    pub fn options(&self) -> &SseOptions {
        &self.options
    }

    /// The `Content-Type` this reader accepts, as checked by [`open`](Self::open) and by the
    /// static [`Sse`](super::Sse) format: exactly one field, `text/event-stream`, a `charset`
    /// absent or `utf-8` (or `us-ascii`) declared once. There is no media policy: the check is the
    /// same whatever the options. For a caller that opens responses itself, then reads the body
    /// with [`read_for`](Self::read_for). It reads the head only; the status is not checked.
    ///
    /// # Errors
    ///
    /// The refusal, with a static label (the one an [`OpenError`] of kind `Metadata` carries).
    pub fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_head(head)
    }

    /// Opens a response ([`Openable`]: the [`Incoming`](concord_core::Incoming) body of an
    /// endpoint declared `-> stream`, a [`Received`](concord_core::Received) response, or a native
    /// response): requires a 2xx status and exactly one `Content-Type`, `text/event-stream` with
    /// a `charset` absent or `utf-8` ([`check_head`](Self::check_head)), before polling the body.
    /// A refusal keeps the response unread ([`OpenError::into_response`]).
    ///
    /// # Errors
    ///
    /// An [`OpenError`] of kind `Status` or `Metadata`.
    pub fn open(&self, response: impl Openable) -> Result<SseEvents, OpenError> {
        let (response, operation) = open_input(response, check_head)?;
        Ok(self.events(response, operation))
    }

    /// The events of a body whose head a static format already checked (`Incremental`).
    pub(super) fn open_checked(&self, body: concord_core::Incoming) -> SseEvents {
        let (response, operation) = body.into_parts();
        self.events(response, operation.map(|op| op.name()))
    }

    /// Reads any byte source, without status or head checks.
    pub fn read<S: ByteSource>(&self, source: S) -> SseEvents<S> {
        self.events(source, None)
    }

    /// [`read`](Self::read), naming the operation in every later error (a static label the SDK
    /// chooses; never derived from the URL), as `open` does for a declared endpoint. No status or
    /// head check: the caller made them — an [`Incoming`](concord_core::Incoming) body is a 2xx,
    /// and [`check_head`](Self::check_head) applies the head rules of `open`. Read through the
    /// `Incoming` itself (it is a [`ByteSource`]), a broken body keeps Concord's own
    /// `Transport` error ([`ReadError::into_source_unredacted`]).
    pub fn read_for<S: ByteSource>(&self, source: S, operation: &'static str) -> SseEvents<S> {
        self.events(source, Some(operation))
    }

    fn events<S: ByteSource>(&self, source: S, operation: Option<&'static str>) -> SseEvents<S> {
        SseEvents {
            driver: Driver::new(source, operation).with_totals(None, self.options.max_events),
            parser: Parser::new(self.options.max_event_bytes),
        }
    }
}

/// The response head check of [`SseReader::open`], [`SseReader::check_head`] and
/// [`Sse`](super::Sse).
pub(super) fn check_head(head: &ResponseHead<'_>) -> Result<(), CodecError> {
    let Some(found) = media::single(head)? else {
        return Err(CodecError::new("the response has no Content-Type"));
    };
    if !found
        .essence
        .essence_str()
        .eq_ignore_ascii_case("text/event-stream")
    {
        return Err(CodecError::new(
            "the response Content-Type is not text/event-stream",
        ));
    }
    media::check_charset(
        &found.charsets,
        "the response declares a charset other than utf-8",
    )
}

/// The events of one `text/event-stream` body, read as chunks arrive (from [`SseReader::open`],
/// [`SseReader::read`], or the `send()` of an endpoint declared `-> stream Event` with
/// `response Sse;`).
///
/// Nothing runs ahead of the caller: each [`next`](Self::next) reads only the chunks needed for
/// one event. It retains the current chunk, the event being assembled (bounded by
/// `max_event_bytes`) and the last event ID, never a queue of events. Dropping it drops the
/// source (a response's connection is closed, not drained). It does not reconnect: resuming with
/// `Last-Event-ID` ([`last_event_id`](Self::last_event_id)) after [`retry`](Self::retry) is the
/// SDK's or the application's policy.
///
/// `Debug` shows counts and the position, never content.
pub struct SseEvents<S = reqwest::Response> {
    driver: Driver<S>,
    parser: Parser,
}

impl<S: ByteSource> SseEvents<S> {
    /// The next event; `None` at the end of input and after a terminal error (fused). Every error
    /// is terminal.
    ///
    /// Cancel-safe when the source is ([`ByteSource`], "Cancellation"; `reqwest::Response` and
    /// [`Chunks`](crate::stream::Chunks) are): dropping the returned future then loses no byte (a
    /// partly received line or event stays buffered for the next call, the rest of the chunk
    /// pending). With any other source, [`into_stream`](Self::into_stream) keeps a pending read,
    /// the source's own future included, across a dropped `next()`. Work per poll is bounded: it
    /// interprets at most 64 KiB of input between two yields to the executor (comments, blank
    /// lines, a long event, one large chunk), and yields after every 64 empty chunks from the
    /// source.
    ///
    /// An error's [`position`](ReadError::position) does not depend on how the input was cut:
    /// through the end of the faulting line (invalid UTF-8), up to the first byte past
    /// `max_event_bytes`, through the dispatching line end of the event `max_events` refused, all
    /// of the input (an end inside a line or an event); a transport failure counts what the source
    /// delivered before it.
    pub async fn next(&mut self) -> Option<Result<Event, ReadError>> {
        if self.driver.is_fused() {
            return None;
        }
        let mut work = 0usize;
        loop {
            if self.driver.input().is_empty() && !self.driver.at_eof() {
                match self.driver.refill().await {
                    Ok(_) => continue,
                    Err(e) => return Some(Err(e)),
                }
            }
            if work >= SKIP_BUDGET {
                work = 0;
                yield_now().await;
            }
            // At most `SKIP_BUDGET` input bytes between two yields; the rest of the chunk stays
            // in the driver. `at_eof` holds only once the chunk is used up.
            let input = self.driver.input();
            let input = &input[..input.len().min(SKIP_BUDGET - work)];
            let eof = self.driver.at_eof();
            let (used, step) = match self.parser.feed(input, eof) {
                Ok(step) => step,
                Err(fault) => {
                    // The input before the fault counts, wherever the chunk ended.
                    self.driver.consume(fault.offset);
                    return Some(Err(self.driver.error(fault.kind, *fault.codec)));
                }
            };
            work += used;
            self.driver.consume(used);
            match step {
                Step::NeedMore => {}
                Step::End => {
                    self.driver.finish();
                    return None;
                }
                Step::Event(event) => {
                    // A refused event leaves the last event ID where it was.
                    if let Err(e) = self.driver.check_record_total() {
                        return Some(Err(e));
                    }
                    self.parser.commit(&event);
                    self.driver.delivered();
                    return Some(Ok(event));
                }
            }
        }
    }

    /// Whether the input ended cleanly at an event boundary ([`Completion::EndOfInput`]), a
    /// terminal error ended the reader (`Failed`), or events may follow (`Reading`). Only
    /// `EndOfInput` proves every event was read; stopping early or dropping the reader is not an
    /// end of input.
    pub fn completion(&self) -> Completion {
        self.driver.completion()
    }

    /// Events delivered and input bytes consumed so far.
    pub fn position(&self) -> Position {
        self.driver.position()
    }

    /// The last event ID string: the most recent valid `id` value as of the last blank line
    /// (empty if none) that either dispatched nothing (an `id` without `data`) or dispatched an
    /// event this reader delivered. An `id` received in an event that has not been completed yet,
    /// or in the event `max_events` refused, does not count, so this is a safe `Last-Event-ID`
    /// for resuming: no event after it was skipped.
    pub fn last_event_id(&self) -> &str {
        self.parser.last_event_id()
    }

    /// The last valid `retry` field (ASCII digits, in milliseconds; a value past `u64`
    /// saturates), `None` if none arrived. It is applied as soon as its line is read.
    pub fn retry(&self) -> Option<Duration> {
        self.parser.retry()
    }

    /// The source, as the reader left it. Bytes the reader already took from it (the event being
    /// assembled, the pending chunk remainder, delivered events) are not in it any more: a
    /// response recovered this way is partially consumed, never unread.
    pub fn into_source(self) -> S {
        self.driver.into_source()
    }
}

impl<S: ByteSource + 'static> SseEvents<S> {
    /// This reader as an owning standard [`Stream`](futures_core::Stream) of
    /// `Result<Event, ReadError>` ([`RecordStream`]): no read-ahead, cancellation-safe, errors and
    /// completion unchanged.
    pub fn into_stream(self) -> RecordStream<Self> {
        RecordStream::new(self)
    }
}

impl<S: ByteSource> fmt::Debug for SseEvents<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SseEvents")
            .field("operation", &self.driver.operation())
            .field("position", &self.driver.position())
            .field("lines", &self.parser.lines())
            .finish_non_exhaustive()
    }
}

impl<S> crate::stream::RecordSealed for SseEvents<S> {}

impl<S: ByteSource + 'static> crate::stream::RecordReader for SseEvents<S> {
    type Record = Event;

    fn next_record(
        &mut self,
    ) -> impl std::future::Future<Output = Option<Result<Event, ReadError>>> + Send {
        self.next()
    }

    fn record_position(&self) -> Position {
        self.position()
    }

    fn record_operation(&self) -> Option<&'static str> {
        self.driver.operation()
    }

    fn record_completion(&self) -> Completion {
        self.driver.completion()
    }
}
