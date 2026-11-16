//! Bounded record framing and safe errors after a response head.

use concord_core::bytes::Bytes;
use concord_core::http::StatusCode;
use concord_core::read::{Incoming, ReadError, Reader, StreamError};
use concord_core::{ByteStream, Headers};
use futures_core::Stream;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// A response head and a stream of records, ending after its first error.
///
/// `try_next` and `Stream` drive the same parser; either can be used between pulls.
/// Dropping this stream releases its body and any concurrency permit. A pending
/// pull retains framing state; dropping a pull never dispatches or reconnects.
/// Parser storage is O(the configured bound), plus one transport-owned chunk and
/// the record currently being decoded. The transport's chunk allocation, the
/// deserializer's allocations and records retained by the caller are additional.
///
/// The call timeout continues while reading, through the underlying byte stream.
/// It bounds pending body waits, not the emission of records already buffered.
/// The idle clock watches incoming bytes, including comments, rather than records.
/// No post-head error causes an engine retry or reconnection.
/// One poll does bounded framing work (at most 16 KiB of incoming bytes and 64 chunks),
/// preserving state and yielding when that budget is exhausted.
///
/// ```no_run
/// use concord::read::{Incoming, Reader, RecordStream, Sse, SseEvent};
/// fn accepts_stream<S: futures_core::Stream<Item = Result<SseEvent, concord::read::RecordError>>>() {}
/// accepts_stream::<RecordStream<SseEvent>>();
/// async fn read_events(body: Incoming) -> Result<(), Box<dyn std::error::Error>> {
///     let mut events: RecordStream<SseEvent> = Sse::new(8192).read(body).await?;
///     let status = events.status();
///     let content_type = events.headers().get("content-type");
///     while let Some(event) = events.try_next().await? { let _data = event.data; }
///     Ok(())
/// }
/// ```
#[must_use = "pull records from the stream or drop it to release the body"]
pub struct RecordStream<T> {
    records: Option<Box<dyn Pull<T> + Send>>,
    status: StatusCode,
    headers: Headers,
}

impl<T> RecordStream<T> {
    /// Returns the status retained at handoff, including after EOF or a framing error.
    ///
    /// ```no_run
    /// fn status<T>(records: &concord::read::RecordStream<T>) -> concord::http::StatusCode {
    ///     records.status()
    /// }
    /// ```
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response headers retained at handoff. `Debug` prints names only.
    ///
    /// ```no_run
    /// fn content_type<T>(records: &concord::read::RecordStream<T>) -> Option<&str> {
    ///     records.headers().get("content-type")
    /// }
    /// ```
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    /// Reads one record, `None` at EOF. The first error is returned once and ends the stream.
    /// A [`RecordError`] is returned directly, never inside a `concord::Error`.
    /// Dropping this future, even before it is polled, preserves partial framing
    /// and stops the idle timer, like [`ByteStream::try_next`].
    /// The call deadline continues to run.
    ///
    /// ```no_run
    /// async fn first<T>(mut records: concord::read::RecordStream<T>)
    ///     -> Result<Option<T>, concord::read::RecordError>
    /// {
    ///     records.try_next().await
    /// }
    /// ```
    pub fn try_next(&mut self) -> impl Future<Output = Result<Option<T>, RecordError>> + Send + '_ {
        struct Wait<'a, T>(&'a mut RecordStream<T>);
        impl<T> Future for Wait<'_, T> {
            type Output = Result<Option<T>, RecordError>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                Pin::new(&mut *self.0).poll_next(cx).map(Option::transpose)
            }
        }
        impl<T> Drop for Wait<'_, T> {
            fn drop(&mut self) {
                if let Some(records) = &mut self.0.records {
                    records.cancel_wait();
                }
            }
        }
        Wait(self)
    }
}

impl<T> fmt::Debug for RecordStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordStream")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

/// As with [`ByteStream`]'s `Stream` implementation, a pending byte wait's idle
/// timer includes time between polls. Dropping an outer `next()` future cannot
/// be observed here; use [`RecordStream::try_next`] to stop that timer on drop.
impl<T> Stream for RecordStream<T> {
    type Item = Result<T, RecordError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(records) = &mut this.records else {
            return Poll::Ready(None);
        };
        let result = Pin::new(records.as_mut()).poll_next(cx);
        if matches!(result, Poll::Ready(None | Some(Err(_)))) {
            // Release the body and all partial framing immediately, even if the
            // caller keeps the exhausted stream to inspect its response head.
            this.records = None;
        }
        result
    }
}

/// A terminal framing or body error, with no record bytes or decoder message.
///
/// `Stream` keeps the entire transport failure, including its endpoint, attempt,
/// status and headers; it is also returned by `source()`. Invalid UTF-8 retains
/// its value-free standard cause. Decode failures retain only safe coordinates,
/// following the core's rule that a decoder message must not enter a source chain.
/// Record positions and line numbers are one-based. [`RecordError::Decode`] has
/// no `source()`, because the decoder's message can quote the record; its column
/// is the decoder's column and can be zero at the start of a line.
///
/// `RecordError` is returned directly by [`RecordStream::try_next`] and its
/// [`Stream`] implementation, never inside a `concord::Error`.
/// A pre-head call error is separate and keeps its usual core source.
///
/// ```no_run
/// use concord::read::RecordError;
/// use std::error::Error as _;
/// async fn first<T>(records: &mut concord::read::RecordStream<T>)
///     -> Result<Option<T>, RecordError>
/// {
///     records.try_next().await
/// }
/// fn inspect(error: &RecordError) {
///     match error {
///         RecordError::Stream(body) => { let _timed_out = body.is_timeout(); }
///         RecordError::Decode { record, line, column, .. } => { let _place = (record, line, column); }
///         _ => {}
///     }
///     let cause = error.source();
///     let safe = format!("{error}: {error:?}");
/// }
/// ```
#[non_exhaustive]
pub enum RecordError {
    /// An unchanged error from the underlying clock-carrying byte stream.
    Stream(StreamError),
    /// A line or complete event block exceeded its configured byte bound.
    #[non_exhaustive]
    TooLarge {
        /// Configured bound, without retaining any record content.
        limit: usize,
    },
    /// A record contained invalid UTF-8. No lossy replacement is performed.
    #[non_exhaustive]
    Utf8 {
        /// One-based line record or event block being parsed.
        record: usize,
        /// Original UTF-8 failure, which contains offsets but no response bytes.
        source: std::str::Utf8Error,
    },
    /// A JSON value did not decode as the requested type.
    /// Has no `source()`: the decoder's message can quote the record.
    #[non_exhaustive]
    Decode {
        /// One-based JSON line when known; `None` for a standalone event helper.
        record: Option<usize>,
        /// One-based line within that record's JSON text.
        line: usize,
        /// Decoder column within that JSON line; zero is possible at its start.
        column: usize,
    },
}

impl fmt::Debug for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stream(error) => f.debug_tuple("Stream").field(error).finish(),
            Self::TooLarge { .. } => f.write_str("TooLarge"),
            Self::Utf8 { .. } => f.write_str("Utf8"),
            Self::Decode { .. } => f.write_str("Decode"),
        }
    }
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stream(error) => fmt::Display::fmt(error, f),
            Self::TooLarge { .. } => f.write_str("record exceeds its byte limit"),
            Self::Utf8 { .. } => f.write_str("record is not UTF-8"),
            Self::Decode { .. } => f.write_str("record does not decode as the requested type"),
        }
    }
}

impl std::error::Error for RecordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Stream(error) => Some(error),
            Self::Utf8 { source, .. } => Some(source),
            Self::TooLarge { .. } | Self::Decode { .. } => None,
        }
    }
}

impl From<StreamError> for RecordError {
    fn from(error: StreamError) -> Self {
        Self::Stream(error)
    }
}

/// Reads one bounded JSON value per line (feature `json`).
///
/// The bound counts bytes before LF, including an optional CR. LF and CRLF are
/// accepted, as is a final line without LF. An empty LF or CRLF line is a
/// [`RecordError::Decode`] error at every bound, including zero. UTF-8 is checked
/// before decoding. No content-type requirement or empty-line skipping
/// is inferred; zero permits no nonempty record. Framing errors are stream items.
///
/// A watch can model an API error as an ordinary typed record:
///
/// ```no_run
/// use concord::read::{JsonLines, RecordStream};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Pod { name: String }
/// #[derive(Deserialize)]
/// struct Status { code: u16 }
/// #[derive(Deserialize)]
/// #[serde(tag = "type", content = "object")]
/// enum WatchEvent {
///     #[serde(rename = "ADDED")] Added(Pod),
///     #[serde(rename = "ERROR")] Error(Status),
/// }
/// concord::api! {
///     client Cluster {
///         base "https://cluster.example.test";
///         GET watch() -> RecordStream<WatchEvent> {
///             path "pods";
///             query { "watch" = true; }
///             read JsonLines::new(64 * concord::KIB);
///         }
///     }
/// }
/// async fn watch() -> Result<(), Box<dyn std::error::Error>> {
///     let cluster = Cluster::new();
///     let mut records = cluster.watch().await?;
///     while let Some(event) = records.try_next().await? {
///         match event {
///             WatchEvent::Added(pod) => { let _name = pod.name; }
///             WatchEvent::Error(status) => { let _code = status.code; break; }
///         }
///     }
///     Ok(())
/// }
/// ```
#[cfg(feature = "json")]
#[derive(Clone, Copy)]
pub struct JsonLines {
    max_record_bytes: usize,
}

#[cfg(feature = "json")]
impl JsonLines {
    /// Sets the maximum bytes of one line; there is no default record bound.
    /// Choose the largest record the API documents, with a margin.
    /// Empty LF and CRLF lines are [`RecordError::Decode`] errors even at zero.
    ///
    /// ```no_run
    /// let reader = concord::read::JsonLines::new(64 * concord::KIB);
    /// ```
    #[must_use]
    pub fn new(max_record_bytes: usize) -> Self {
        Self { max_record_bytes }
    }
}

#[cfg(feature = "json")]
impl fmt::Debug for JsonLines {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JsonLines")
    }
}

#[cfg(feature = "json")]
impl<T: serde::de::DeserializeOwned + Send + 'static> Reader<RecordStream<T>> for JsonLines {
    async fn read(&self, body: Incoming) -> Result<RecordStream<T>, ReadError> {
        let mut record = 0usize;
        Ok(framed(body, self.max_record_bytes, false, move |line| {
            record = record.saturating_add(1);
            std::str::from_utf8(&line).map_err(|source| RecordError::Utf8 { record, source })?;
            decode(&line, Some(record)).map(Some)
        }))
    }
}

/// A server-sent event, with metadata snapshots and unmodified joined data text.
///
/// `event` defaults to `message`. `id` and `retry` persist between events; they are
/// snapshots at dispatch, not a complete reconnect state when control-only blocks
/// arrive. All text is absent from `Debug`. The reader interprets no provider event
/// name or completion marker.
///
/// ```no_run
/// use concord::read::SseEvent;
/// let mut event = SseEvent::new("hello");
/// event.event = "delta".into();
/// event.id = "42".into();
/// event.retry = Some(std::time::Duration::from_secs(1));
/// let delay = event.retry;
/// let safe = format!("{event:?}");
/// ```
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The event type, or `message` when no type was supplied.
    pub event: String,
    /// `data` fields joined with LF, with the final appended LF removed.
    pub data: String,
    /// Last valid event ID, initially empty; an empty `id` resets it.
    pub id: String,
    /// Last valid retry duration (wire milliseconds); no reconnect is performed.
    /// Fields are ignored if empty, if they contain anything but ASCII digits,
    /// or if their millisecond value exceeds `u64::MAX`.
    pub retry: Option<Duration>,
}

impl SseEvent {
    /// Creates a standalone `message` event with this data, an empty ID and no retry hint.
    ///
    /// ```no_run
    /// let mut event = concord::read::SseEvent::new("hello");
    /// event.event = "delta".into();
    /// ```
    #[must_use]
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event: "message".into(),
            data: data.into(),
            id: String::new(),
            retry: None,
        }
    }

    /// Decodes `data` as JSON (feature `json`), discarding the decoder's message.
    ///
    /// A helper failure does not itself stop a separately held `RecordStream`;
    /// propagate it or drop the stream to end an application's typed adapter.
    /// The record position is `None`: this event does not know its stream ordinal.
    /// A decode error has no `source()` and keeps the decoder's column, which can be zero.
    ///
    /// ```no_run
    /// use concord::read::SseEvent;
    /// use serde::Deserialize;
    /// #[derive(Deserialize)]
    /// struct Delta { text: String }
    /// fn delta(event: &SseEvent) -> Result<String, concord::read::RecordError> {
    ///     Ok(event.json::<Delta>()?.text)
    /// }
    /// ```
    #[cfg(feature = "json")]
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, RecordError> {
        decode(self.data.as_bytes(), None)
    }
}

impl fmt::Debug for SseEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SseEvent { .. }")
    }
}

/// Reads bounded server-sent events, without reconnecting or interpreting their data.
///
/// Accepts LF, CRLF and lone CR; an initial UTF-8 BOM is skipped before framing
/// and does not count toward the bound. A blank line dispatches;
/// EOF discards an unfinished event. Comments and unknown fields are ignored but
/// count toward the bound. IDs with NUL are ignored. A retry field is ignored
/// if empty, if it contains anything but ASCII digits, or if its millisecond
/// value exceeds `u64::MAX`.
/// Invalid UTF-8 is a terminal error. Empty `data:` still dispatches an event.
///
/// The bound is the sum of each nonblank line's bytes plus one delimiter byte,
/// including ignored fields and comments. CRLF counts as one delimiter; the blank
/// dispatch line does not count. The bound is checked as bytes arrive, including
/// the delimiter allowance for an unfinished nonblank line. Zero
/// permits no nonblank event block. Storage is bounded as described by `RecordStream`.
///
/// Provider error and completion events belong to the application. The JSON
/// mapping in this example enables `json`; raw `Sse` needs no feature.
///
/// ```no_run
/// use concord::read::{Sse, SseEvent, RecordStream};
/// use serde::Deserialize;
/// #[derive(Deserialize)]
/// struct Delta { text: String }
/// #[derive(Deserialize)]
/// struct Problem { code: u16 }
/// concord::api! {
///     client Events {
///         base "https://events.example.test";
///         GET events() -> RecordStream<SseEvent> {
///             path "events"; read Sse::new(16 * concord::KIB);
///         }
///     }
/// }
/// #[derive(Debug)]
/// enum WatchError { Http(concord::Error), Record(concord::read::RecordError), Api(u16) }
/// async fn watch() -> Result<(), WatchError> {
///     let api = Events::new();
///     let mut events = api.events().await.map_err(WatchError::Http)?;
///     while let Some(event) = events.try_next().await.map_err(WatchError::Record)? {
///         match event.event.as_str() {
///             "delta" => {
///                 let delta = event.json::<Delta>().map_err(WatchError::Record)?;
///                 let _text = delta.text;
///             }
///             "error" => {
///                 let problem = event.json::<Problem>().map_err(WatchError::Record)?;
///                 return Err(WatchError::Api(problem.code));
///             }
///             "done" => break,
///             _ => {}
///         }
///     }
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Sse {
    max_event_bytes: usize,
}

impl Sse {
    /// Sets the maximum normalized wire bytes of one event block, excluding an initial BOM.
    /// Choose the largest event the API documents, with a margin.
    ///
    /// ```no_run
    /// let reader = concord::read::Sse::new(16 * concord::KIB);
    /// ```
    #[must_use]
    pub fn new(max_event_bytes: usize) -> Self {
        Self { max_event_bytes }
    }
}

impl fmt::Debug for Sse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sse")
    }
}

impl Reader<RecordStream<SseEvent>> for Sse {
    async fn read(&self, body: Incoming) -> Result<RecordStream<SseEvent>, ReadError> {
        let mut events = Events::new();
        Ok(framed(body, self.max_event_bytes, true, move |line| {
            events.line(&line)
        }))
    }
}

#[cfg(feature = "json")]
fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    record: Option<usize>,
) -> Result<T, RecordError> {
    // Decoder messages and their sources may quote response data. Keep neither.
    serde_json::from_slice(bytes).map_err(|error| RecordError::Decode {
        record,
        line: error.line(),
        column: error.column(),
    })
}

// The byte stream owns its pending transport operation. Framing and field state
// live here, never in a temporary pull future. Polling cannot yield between
// consuming a byte and retaining its effect.
trait Pull<T>: Stream<Item = Result<T, RecordError>> + Unpin {
    fn cancel_wait(&mut self);
}

type Parse<T> = dyn FnMut(Vec<u8>) -> Result<Option<T>, RecordError> + Send;

struct Framed<T> {
    lines: Lines,
    parse: Box<Parse<T>>,
}

fn framed<T: 'static>(
    body: Incoming,
    max: usize,
    sse: bool,
    parse: impl FnMut(Vec<u8>) -> Result<Option<T>, RecordError> + Send + 'static,
) -> RecordStream<T> {
    RecordStream {
        status: body.status(),
        headers: body.headers().clone(),
        records: Some(Box::new(Framed {
            lines: Lines::new(body.stream(), max, sse),
            parse: Box::new(parse),
        })),
    }
}

impl<T> Pull<T> for Framed<T> {
    fn cancel_wait(&mut self) {
        // ByteStream's public pull guard resets its idle timer on drop, even
        // when unpolled. Its body, call clock and buffered bytes stay in place.
        drop(self.lines.input.try_next());
    }
}

impl<T> Stream for Framed<T> {
    type Item = Result<T, RecordError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let mut work = Work {
            bytes: 16_384,
            chunks: 64,
        };
        loop {
            let line = match this.lines.poll_line(cx, &mut work) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(Ok(None)) => return Poll::Ready(None),
                Poll::Ready(Ok(Some(line))) => line,
            };
            match (this.parse)(line) {
                Ok(None) => {}
                Ok(Some(record)) => return Poll::Ready(Some(Ok(record))),
                Err(error) => return Poll::Ready(Some(Err(error))),
            }
        }
    }
}

// Shared by every line and body read in one outer poll, including ignored SSE
// blocks and empty chunks. Neither server-controlled loop can reset the budget.
struct Work {
    bytes: usize,
    chunks: usize,
}

fn spend(remaining: &mut usize, cx: &mut Context<'_>) -> bool {
    if *remaining == 0 {
        cx.waker().wake_by_ref();
        false
    } else {
        *remaining = remaining.saturating_sub(1);
        true
    }
}

// Line framing: LF for JSON Lines, LF/CRLF/CR for SSE, a bounded
// unfinished line, and at most one transport-owned chunk retained at a time.
struct Lines {
    input: ByteStream,
    chunk: Bytes,
    pos: usize,
    line: Vec<u8>,
    max: usize,
    sse: bool,
    skip_lf: bool,
    bom: Option<usize>,
    empty_cr: bool,
    event_size: usize,
}

const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

impl Lines {
    fn new(input: ByteStream, max: usize, sse: bool) -> Self {
        Self {
            input,
            chunk: Bytes::new(),
            pos: 0,
            line: Vec::new(),
            max,
            sse,
            skip_lf: false,
            bom: sse.then_some(0),
            empty_cr: false,
            event_size: 0,
        }
    }

    // A partial BOM that stops matching (or reaches EOF) is ordinary input.
    // Matching bytes are held as a count, outside the bounded record buffer.
    fn keep_bom_prefix(&mut self) -> Result<(), RecordError> {
        if let Some(matched) = self.bom.take() {
            let prefix = UTF8_BOM.get(..matched).unwrap_or_default();
            if prefix.len() > self.max.saturating_sub(self.line.len()) {
                return Err(RecordError::TooLarge { limit: self.max });
            }
            self.count_event_bytes(prefix.len())?;
            self.line.extend_from_slice(prefix);
        }
        Ok(())
    }

    // A nonblank line spends its normalized delimiter as soon as its first
    // byte arrives. Count ignored fields too, and reset only at a blank line.
    fn count_event_bytes(&mut self, bytes: usize) -> Result<(), RecordError> {
        if self.sse && bytes != 0 {
            self.event_size = bytes
                .checked_add(usize::from(self.line.is_empty()))
                .and_then(|extra| self.event_size.checked_add(extra))
                .filter(|&size| size <= self.max)
                .ok_or(RecordError::TooLarge { limit: self.max })?;
        }
        Ok(())
    }

    fn poll_line(
        &mut self,
        cx: &mut Context<'_>,
        work: &mut Work,
    ) -> Poll<Result<Option<Vec<u8>>, RecordError>> {
        loop {
            while let Some(byte) = self.chunk.get(self.pos).copied() {
                if !spend(&mut work.bytes, cx) {
                    return Poll::Pending;
                }
                self.pos = self.pos.saturating_add(1);
                if let Some(matched) = self.bom {
                    if UTF8_BOM.get(matched) == Some(&byte) {
                        let matched = matched.saturating_add(1);
                        self.bom = (matched < UTF8_BOM.len()).then_some(matched);
                        continue;
                    }
                    if let Err(error) = self.keep_bom_prefix() {
                        return Poll::Ready(Err(error));
                    }
                }
                if std::mem::take(&mut self.empty_cr) && byte != b'\n' {
                    return Poll::Ready(Err(RecordError::TooLarge { limit: self.max }));
                }
                if std::mem::take(&mut self.skip_lf) && byte == b'\n' {
                    continue;
                }
                if byte == b'\n' || (self.sse && byte == b'\r') {
                    self.skip_lf = self.sse && byte == b'\r';
                    let mut line = std::mem::take(&mut self.line);
                    if self.sse && line.is_empty() {
                        self.event_size = 0;
                    }
                    if !self.sse && line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return Poll::Ready(Ok(Some(line)));
                }
                if self.line.len() == self.max {
                    if !self.sse && self.max == 0 && byte == b'\r' {
                        // At zero, retain only delimiter state until LF confirms
                        // an empty CRLF line, which must reach the decoder.
                        self.empty_cr = true;
                        continue;
                    }
                    return Poll::Ready(Err(RecordError::TooLarge { limit: self.max }));
                }
                if let Err(error) = self.count_event_bytes(1) {
                    return Poll::Ready(Err(error));
                }
                self.line.push(byte);
            }
            // Release an exhausted chunk before waiting for the next one.
            self.chunk = Bytes::new();
            self.pos = 0;
            if !spend(&mut work.chunks, cx) {
                return Poll::Pending;
            }
            match Pin::new(&mut self.input).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Err(error.into())),
                Poll::Ready(Some(Ok(chunk))) => self.chunk = chunk,
                Poll::Ready(None) => {
                    if let Err(error) = self.keep_bom_prefix() {
                        return Poll::Ready(Err(error));
                    }
                    if self.empty_cr {
                        return Poll::Ready(Err(RecordError::TooLarge { limit: self.max }));
                    }
                    return Poll::Ready(Ok(
                        (!self.line.is_empty()).then(|| std::mem::take(&mut self.line))
                    ));
                }
            }
        }
    }
}

struct Events {
    record: usize,
    data: String,
    event: String,
    id: String,
    retry: Option<Duration>,
}

impl Events {
    fn new() -> Self {
        Self {
            record: 1,
            data: String::new(),
            event: String::new(),
            id: String::new(),
            retry: None,
        }
    }

    fn line(&mut self, bytes: &[u8]) -> Result<Option<SseEvent>, RecordError> {
        let line = std::str::from_utf8(bytes).map_err(|source| RecordError::Utf8 {
            record: self.record,
            source,
        })?;
        if line.is_empty() {
            self.record = self.record.saturating_add(1);
            let event = std::mem::take(&mut self.event);
            if self.data.is_empty() {
                return Ok(None);
            }
            self.data.pop(); // Remove the LF appended after the final data field.
            return Ok(Some(SseEvent {
                event: if event.is_empty() {
                    "message".into()
                } else {
                    event
                },
                data: std::mem::take(&mut self.data),
                id: self.id.clone(),
                retry: self.retry,
            }));
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
            }
            "event" => self.event = value.into(),
            "id" if !value.contains('\0') => self.id = value.into(),
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                if let Ok(ms) = value.parse() {
                    self.retry = Some(Duration::from_millis(ms));
                }
            }
            _ => {}
        }
        Ok(None)
    }
}
