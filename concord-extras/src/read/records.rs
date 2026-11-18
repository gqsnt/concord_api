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

mod mapped;
pub use mapped::{Mapped, Step};

/// The status and headers of a response, and a stream of its records that ends after
/// the first error.
///
/// `E` is the error of the stream. A reader of records gives [`RecordError`], the default:
/// `RecordStream<SseEvent>`. A reader whose records go through a mapping of your own
/// ([`Sse::map`], [`JsonLines::map`]) gives the client's own error, `concord::Error<P>`, so
/// that `?` on the call and `?` on `try_next` meet in one type:
/// `RecordStream<StreamEvent, concord::Error<ApiProblem>>`.
///
/// `try_next` and `Stream` drive the same parser; you can switch from one to the other
/// between records. Dropping this stream releases its body and its place in any cap on
/// concurrency. A pending read keeps the parser's state; dropping it sends no request
/// and does not reconnect.
/// Parser storage is O(the configured bound), plus one transport-owned chunk and
/// the record currently being decoded. The transport's chunk allocation, the
/// deserializer's allocations and records retained by the caller are additional.
///
/// The call timeout keeps running while you read, through the underlying byte stream.
/// It bounds the waits for the body, not the return of records already buffered. The
/// idle timeout watches incoming bytes, including comments, not records. No error after
/// the response head makes the engine retry or reconnect. One poll frames at most
/// 16 KiB of incoming bytes and 64 chunks; if that budget runs out before a record is
/// ready, it keeps its state and yields.
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
pub struct RecordStream<T, E = RecordError> {
    records: Option<Box<dyn Pull<T, E> + Send>>,
    status: StatusCode,
    headers: Headers,
}

impl<T, E> RecordStream<T, E> {
    /// Returns the response status, kept when the stream was created; it stays available
    /// after the end of the body or a framing error.
    ///
    /// ```no_run
    /// fn status<T>(records: &concord::read::RecordStream<T>) -> concord::http::StatusCode {
    ///     records.status()
    /// }
    /// ```
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the response headers, kept when the stream was created. `Debug` prints
    /// their names only.
    ///
    /// ```no_run
    /// fn content_type<T>(records: &concord::read::RecordStream<T>) -> Option<&str> {
    ///     records.headers().get("content-type")
    /// }
    /// ```
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    /// Reads the next record, or `None` at the end of the body.
    ///
    /// The first error is returned once and ends the stream. With the default `E`, a
    /// [`RecordError`] is returned directly, never inside a `concord::Error`; a mapped
    /// stream returns the client's own `concord::Error<P>` instead ([`Step`]). Dropping
    /// this future, even before it is polled, keeps the partial record and stops the idle
    /// timer, like [`ByteStream::try_next`]. The call timeout keeps running.
    ///
    /// ```no_run
    /// async fn first<T>(mut records: concord::read::RecordStream<T>)
    ///     -> Result<Option<T>, concord::read::RecordError>
    /// {
    ///     records.try_next().await
    /// }
    /// ```
    pub fn try_next(&mut self) -> impl Future<Output = Result<Option<T>, E>> + Send + '_ {
        struct Wait<'a, T, E>(&'a mut RecordStream<T, E>);
        impl<T, E> Future for Wait<'_, T, E> {
            type Output = Result<Option<T>, E>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                Pin::new(&mut *self.0).poll_next(cx).map(Option::transpose)
            }
        }
        impl<T, E> Drop for Wait<'_, T, E> {
            fn drop(&mut self) {
                if let Some(records) = &mut self.0.records {
                    records.cancel_wait();
                }
            }
        }
        Wait(self)
    }
}

impl<T, E> fmt::Debug for RecordStream<T, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordStream")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

/// As with the `Stream` implementation of [`ByteStream`], the idle timeout of a pending
/// wait for bytes includes the time between polls. This implementation cannot see an
/// outer `next()` future being dropped; use [`RecordStream::try_next`] when dropping a
/// read must stop that timer.
impl<T, E> Stream for RecordStream<T, E> {
    type Item = Result<T, E>;

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

/// An error of the body or of framing that ends a record stream; it holds no record
/// bytes and no decoder message.
///
/// `Stream` keeps the whole transport error, including its endpoint, attempt, status
/// and headers; `source()` also returns it. `Utf8` keeps the standard error as its
/// cause, which holds no value. `Decode` keeps only safe positions, following the
/// core's rule that a decoder message must not enter a chain of sources.
/// Record positions and line numbers are one-based. [`RecordError::Decode`] has
/// no `source()`, because the decoder's message can quote the record; its column
/// is the decoder's column and can be zero at the start of a line.
///
/// `RecordError` is returned directly by [`RecordStream::try_next`] and its
/// [`Stream`] implementation, never inside a `concord::Error`.
/// An error of the call before the response head is separate, and keeps its usual
/// source from the core.
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
    /// An error of the underlying byte stream, which carries the timeouts, unchanged.
    Stream(StreamError),
    /// A line or complete event block exceeded its configured byte bound.
    #[non_exhaustive]
    TooLarge {
        /// The configured bound; no record content is kept.
        limit: usize,
    },
    /// A record contained invalid UTF-8. No lossy replacement is performed.
    #[non_exhaustive]
    Utf8 {
        /// The one-based position of the line or event block being parsed.
        record: usize,
        /// The original UTF-8 error, which holds offsets but no response bytes.
        source: std::str::Utf8Error,
    },
    /// A JSON value did not decode as the requested type.
    /// Has no `source()`: the decoder's message can quote the record.
    #[non_exhaustive]
    Decode {
        /// The one-based JSON line when known; `None` from the `SseEvent::json` helper.
        record: Option<usize>,
        /// The one-based line within that record's JSON text.
        line: usize,
        /// The decoder's column within that line; it can be zero at the start of the line.
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

/// A reader of one JSON value per line, each line within a byte bound (feature `json`).
///
/// The bound counts the bytes before LF, including an optional CR. LF and CRLF are
/// accepted, as is a last line without LF. An empty LF or CRLF line is a
/// [`RecordError::Decode`] error at every bound, including zero. UTF-8 is checked
/// before decoding. The reader neither requires a content type nor skips empty lines;
/// a bound of zero permits no nonempty record. Framing errors are items of the stream.
///
/// A watch endpoint can describe an error of the API as an ordinary typed record:
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
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
#[derive(Clone, Copy)]
pub struct JsonLines {
    max_record_bytes: usize,
}

#[cfg(feature = "json")]
impl JsonLines {
    /// A reader whose lines may be at most `max_record_bytes` long; there is no default bound.
    ///
    /// Choose the largest record the API documents, with a margin. Empty LF and CRLF
    /// lines are [`RecordError::Decode`] errors, even with a bound of zero.
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

/// A server-sent event: its type, its data joined unchanged, and copies of the last ID
/// and retry delay.
///
/// `event` defaults to `message`. `id` and `retry` carry over from one event to the
/// next; each event holds their values at the time it was dispatched, which is not the
/// whole reconnection state when blocks with only control fields arrive. `Debug` prints
/// no text. The reader gives no meaning to a provider's event names or end markers.
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
    /// The last valid event ID, empty at first; an empty `id` field resets it.
    pub id: String,
    /// The last valid retry delay (sent in milliseconds); the reader never reconnects.
    /// A `retry` field is ignored if it is empty, if it contains anything but ASCII
    /// digits, or if its value in milliseconds exceeds `u64::MAX`.
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
    /// # Errors
    ///
    /// Returns [`RecordError::Decode`] when `data` does not decode as `T`. Its record
    /// position is `None`, because an event does not know its position in the stream.
    /// The error has no `source()` and keeps the decoder's column, which can be zero.
    /// It does not by itself end a `RecordStream` held separately: return it, or drop
    /// the stream, to end your own typed adapter.
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
    #[cfg_attr(docsrs, doc(cfg(feature = "json")))]
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, RecordError> {
        decode(self.data.as_bytes(), None)
    }
}

impl fmt::Debug for SseEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SseEvent { .. }")
    }
}

/// A reader of server-sent events, each within a byte bound; it never reconnects and
/// gives no meaning to their data.
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
/// the delimiter byte counted for an unfinished nonblank line. A bound of zero permits
/// no nonblank event block. Storage is bounded as described for `RecordStream`.
///
/// Error and end events of a provider are for the application to handle. The JSON
/// mapping in this example needs the feature `json`; `Sse` alone needs no feature.
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
    /// A reader whose event blocks may be at most `max_event_bytes`, counted as above and
    /// excluding an initial BOM.
    ///
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
trait Pull<T, E = RecordError>: Stream<Item = Result<T, E>> + Unpin {
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
