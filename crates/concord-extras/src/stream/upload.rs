//! Lazily produced upload bodies: a one-shot record source serialized as the transport asks for
//! bytes.

use std::error::Error as StdError;
use std::fmt;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

use concord_core::bytes::Bytes;
use concord_core::reqwest::header::{CONTENT_TYPE, HeaderValue};
use concord_core::{CodecError, reqwest};
use futures_core::Stream;
use http_body::{Body, Frame, SizeHint};

/// Bytes accumulated before a frame is handed to the transport; also the most records a single
/// poll serializes is bounded by this size.
pub(crate) const FRAME_TARGET: usize = 16 * 1024;

/// A request body and the `Content-Type` that describes it, produced together by a runtime
/// configured writer (a `CsvWriter` whose header/delimiter were chosen per job).
///
/// `Encode` gives a format one static `Content-Type`; a body whose media parameters vary per
/// call cannot use it, so the body and its header travel together here. Apply both:
/// [`apply`](Self::apply) sets them on a native request (from `Prepared::into_parts`), or take them
/// apart with [`into_parts`](Self::into_parts).
#[must_use = "an upload does nothing until its body is sent"]
pub struct Upload {
    body: reqwest::Body,
    content_type: HeaderValue,
}

impl Upload {
    pub(crate) fn new(body: reqwest::Body, content_type: HeaderValue) -> Self {
        Self { body, content_type }
    }

    /// The `Content-Type` matching the body.
    pub fn content_type(&self) -> &HeaderValue {
        &self.content_type
    }

    /// The body and its `Content-Type`.
    pub fn into_parts(self) -> (reqwest::Body, HeaderValue) {
        (self.body, self.content_type)
    }

    /// Sets the body and replaces every `Content-Type` of `request` with the matching one.
    pub fn apply(self, request: &mut reqwest::Request) {
        request
            .headers_mut()
            .insert(CONTENT_TYPE, self.content_type);
        *request.body_mut() = Some(self.body);
    }
}

impl fmt::Debug for Upload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Upload")
            .field("content_type", &self.content_type)
            .field("body", &"<lazy>")
            .finish()
    }
}

/// Serializes records into a byte buffer for a [`RecordBody`].
pub(crate) trait RecordEncoder<T>: Send + Unpin + 'static {
    /// Bytes written before the first record (a header known from configuration).
    fn begin(&mut self, out: &mut Vec<u8>) -> Result<(), CodecError>;
    /// One record (and, for the first one, anything derived from it, such as an inferred
    /// header).
    fn record(&mut self, record: &T, out: &mut Vec<u8>) -> Result<(), CodecError>;
    /// Bytes written after the last record; `records` is how many there were.
    fn finish(&mut self, out: &mut Vec<u8>, records: u64) -> Result<(), CodecError>;
}

/// A record source or its serialization failed while the body was being sent. The transfer
/// fails (reqwest reports a body error); bytes already handed to the transport are not taken
/// back and nothing is retried.
pub struct UploadError {
    label: &'static str,
    records: u64,
    codec: Option<CodecError>,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl UploadError {
    /// The static description.
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// Records serialized (and handed to the body buffer) before the failure.
    pub fn records(&self) -> u64 {
        self.records
    }

    /// The serializer's refusal, if the failure was a record that could not be encoded.
    pub fn codec(&self) -> Option<&CodecError> {
        self.codec.as_ref()
    }

    /// The source's own error, **unredacted**.
    pub fn source_unredacted(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Display for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label)?;
        if let Some(codec) = &self.codec {
            write!(f, ": {codec}")?;
        }
        write!(f, " (after {} records)", self.records)
    }
}

impl fmt::Debug for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadError")
            .field("label", &self.label)
            .field("records", &self.records)
            .field("codec", &self.codec)
            .field("source", &self.source.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// No `source()`: the source's error may quote content.
impl StdError for UploadError {}

enum Phase {
    Begin,
    Records,
    Done,
}

struct State<S, E> {
    source: Pin<Box<S>>,
    encoder: E,
    buffer: Vec<u8>,
    records: u64,
    phase: Phase,
}

/// An `http_body::Body` over a one-shot record source. Nothing happens until the transport polls
/// it: building it polls the source zero times. Each poll serializes records until about
/// [`FRAME_TARGET`] bytes are buffered, the source is pending or it ends; at most one record plus
/// one frame is retained.
///
/// `reqwest::Body::wrap` requires `Sync`; the state sits behind a `Mutex` that is only ever
/// accessed through `get_mut` (no locking), so the source itself need only be `Send`.
pub(crate) struct RecordBody<S, E> {
    state: Mutex<State<S, E>>,
}

impl<S, E> RecordBody<S, E> {
    pub(crate) fn new(source: S, encoder: E) -> Self {
        Self {
            state: Mutex::new(State {
                source: Box::pin(source),
                encoder,
                buffer: Vec::new(),
                records: 0,
                phase: Phase::Begin,
            }),
        }
    }
}

impl<S, T, X, E> RecordBody<S, E>
where
    S: Stream<Item = Result<T, X>> + Send + 'static,
    X: Into<Box<dyn StdError + Send + Sync>>,
    E: RecordEncoder<T>,
    T: 'static,
{
    /// The native body.
    pub(crate) fn into_body(self) -> reqwest::Body {
        reqwest::Body::wrap(self)
    }
}

impl<S, T, X, E> Body for RecordBody<S, E>
where
    S: Stream<Item = Result<T, X>> + Send + 'static,
    X: Into<Box<dyn StdError + Send + Sync>>,
    E: RecordEncoder<T>,
{
    type Data = Bytes;
    type Error = UploadError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, UploadError>>> {
        let state = match self.get_mut().state.get_mut() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let fail = |state: &mut State<S, E>, label, codec, source| {
            state.phase = Phase::Done;
            state.buffer = Vec::new();
            Poll::Ready(Some(Err(UploadError {
                label,
                records: state.records,
                codec,
                source,
            })))
        };
        if let Phase::Begin = state.phase {
            if let Err(e) = state.encoder.begin(&mut state.buffer) {
                return fail(state, "the upload could not begin", Some(e), None);
            }
            state.phase = Phase::Records;
        }
        while let Phase::Records = state.phase {
            if state.buffer.len() >= FRAME_TARGET {
                break;
            }
            match state.source.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(record))) => {
                    if let Err(e) = state.encoder.record(&record, &mut state.buffer) {
                        return fail(
                            state,
                            "an upload record could not be encoded",
                            Some(e),
                            None,
                        );
                    }
                    state.records += 1;
                }
                Poll::Ready(Some(Err(e))) => {
                    return fail(state, "the upload source failed", None, Some(e.into()));
                }
                Poll::Ready(None) => {
                    let records = state.records;
                    if let Err(e) = state.encoder.finish(&mut state.buffer, records) {
                        return fail(state, "the upload could not be completed", Some(e), None);
                    }
                    state.phase = Phase::Done;
                }
                Poll::Pending => {
                    if state.buffer.is_empty() {
                        return Poll::Pending;
                    }
                    break;
                }
            }
        }
        if state.buffer.is_empty() {
            return Poll::Ready(None);
        }
        let frame = Bytes::from(std::mem::take(&mut state.buffer));
        Poll::Ready(Some(Ok(Frame::data(frame))))
    }

    fn is_end_stream(&self) -> bool {
        match self.state.lock() {
            Ok(state) => matches!(state.phase, Phase::Done) && state.buffer.is_empty(),
            Err(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}

/// A [`Stream`] over an iterator of results: a synchronous one-shot source for an upload body.
/// Created by [`from_iter`].
pub struct FromIter<I> {
    iter: I,
}

/// Wraps an iterator of `Result<T, E>` as a one-shot record source (`StreamRows`, `StreamRecords`).
/// The iterator is advanced only when the body is polled.
pub fn from_iter<I>(iter: I) -> FromIter<I::IntoIter>
where
    I: IntoIterator,
{
    FromIter {
        iter: iter.into_iter(),
    }
}

impl<I: Iterator + Unpin> Stream for FromIter<I> {
    type Item = I::Item;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<I::Item>> {
        Poll::Ready(self.iter.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}
