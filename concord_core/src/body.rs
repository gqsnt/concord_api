use crate::stream_body::{AsyncReadByteStream, StreamBody, StreamBodyErrorKind};
use crate::transport::{TransportError, TransportErrorKind};
use bytes::Bytes;
use futures_core::Stream;
use http_body::{Body, Frame, SizeHint};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyDataStream, BodyExt, Empty, Full, StreamBody as UtilStreamBody};
use std::any::Any;
use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Context, Poll};
use tokio::fs::File;
use tokio::io::AsyncRead;

/// Safe categories of failures produced while adapting a request or response body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BodyErrorKind {
    /// An underlying asynchronous reader failed.
    Io,
    /// The source stream or body failed.
    Input,
    /// An adapter was given an invalid configuration.
    InvalidConfiguration,
    /// Exclusive polling state was poisoned or otherwise unavailable.
    ExclusivePoll,
    /// A frame would exceed the configured byte limit.
    LimitExceeded,
    /// A body ended before a declared exact byte length was delivered.
    ExactLengthUnderflow,
    /// A body attempted to deliver bytes beyond a declared exact byte length.
    ExactLengthOverflow,
    /// A producer failed without a more specific safe category.
    Other,
}

/// A body error whose diagnostics never retain producer text or body payloads.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BodyError {
    kind: BodyErrorKind,
    limit: Option<u64>,
    observed: Option<u64>,
}

impl BodyError {
    /// Creates a safe error for an input producer.
    pub const fn input() -> Self {
        Self::new(BodyErrorKind::Input)
    }

    /// Creates a safe error for an unspecified producer failure.
    pub const fn other() -> Self {
        Self::new(BodyErrorKind::Other)
    }

    /// Creates a safe invalid-configuration error.
    pub const fn invalid_configuration() -> Self {
        Self::new(BodyErrorKind::InvalidConfiguration)
    }

    /// Creates a safe byte-limit error with bounded numeric metadata only.
    pub const fn limit_exceeded(limit: u64, observed: u64) -> Self {
        Self {
            kind: BodyErrorKind::LimitExceeded,
            limit: Some(limit),
            observed: Some(observed),
        }
    }

    /// Creates a safe exact-length underflow error.  Lengths are safe numeric
    /// metadata; body contents and producer diagnostics are never retained.
    pub const fn exact_length_underflow(expected: u64, observed: u64) -> Self {
        Self {
            kind: BodyErrorKind::ExactLengthUnderflow,
            limit: Some(expected),
            observed: Some(observed),
        }
    }

    /// Creates a safe exact-length overflow error.
    pub const fn exact_length_overflow(expected: u64, observed: u64) -> Self {
        Self {
            kind: BodyErrorKind::ExactLengthOverflow,
            limit: Some(expected),
            observed: Some(observed),
        }
    }

    /// Returns the safe error category.
    pub const fn kind(self) -> BodyErrorKind {
        self.kind
    }

    /// Returns the configured limit when this is a limit error.
    pub const fn limit(self) -> Option<u64> {
        self.limit
    }

    /// Returns the bounded observed count when this is a limit error.
    pub const fn observed(self) -> Option<u64> {
        self.observed
    }

    const fn new(kind: BodyErrorKind) -> Self {
        Self {
            kind,
            limit: None,
            observed: None,
        }
    }

    fn exclusive_poll() -> Self {
        Self::new(BodyErrorKind::ExclusivePoll)
    }

    fn from_producer<E: Send + 'static>(error: E) -> Self {
        let error: Box<dyn Any + Send> = Box::new(error);
        let error = match error.downcast::<Self>() {
            Ok(error) => return *error,
            Err(error) => error,
        };
        let error = match error.downcast::<crate::stream_body::StreamBodyError>() {
            Ok(error) => return (*error).into(),
            Err(error) => error,
        };
        let error = match error.downcast::<TransportError>() {
            Ok(error) => return (*error).into(),
            Err(error) => error,
        };
        if let Ok(error) = error.downcast::<std::io::Error>() {
            return (*error).into();
        }
        Self::input()
    }
}

impl fmt::Debug for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyError")
            .field("kind", &self.kind)
            .field("limit", &self.limit)
            .field("observed", &self.observed)
            .finish()
    }
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BodyErrorKind::Io => f.write_str("body I/O failure"),
            BodyErrorKind::Input => f.write_str("body input stream failure"),
            BodyErrorKind::InvalidConfiguration => {
                f.write_str("invalid body adapter configuration")
            }
            BodyErrorKind::ExclusivePoll => f.write_str("body exclusive-poll failure"),
            BodyErrorKind::LimitExceeded => write!(
                f,
                "body byte limit exceeded (limit {}, observed {})",
                self.limit.unwrap_or(0),
                self.observed.unwrap_or(0)
            ),
            BodyErrorKind::ExactLengthUnderflow => write!(
                f,
                "body ended before declared exact length (expected {}, observed {})",
                self.limit.unwrap_or(0),
                self.observed.unwrap_or(0)
            ),
            BodyErrorKind::ExactLengthOverflow => write!(
                f,
                "body exceeded declared exact length (expected {}, observed {})",
                self.limit.unwrap_or(0),
                self.observed.unwrap_or(0)
            ),
            BodyErrorKind::Other => f.write_str("body producer failure"),
        }
    }
}

impl Error for BodyError {}

impl From<std::io::Error> for BodyError {
    fn from(_: std::io::Error) -> Self {
        Self::new(BodyErrorKind::Io)
    }
}

impl From<crate::stream_body::StreamBodyError> for BodyError {
    fn from(error: crate::stream_body::StreamBodyError) -> Self {
        match error.kind() {
            StreamBodyErrorKind::Io => Self::new(BodyErrorKind::Io),
            StreamBodyErrorKind::InvalidChunkSize | StreamBodyErrorKind::SizeHint => {
                Self::invalid_configuration()
            }
            StreamBodyErrorKind::Transport => Self::input(),
        }
    }
}

impl From<TransportError> for BodyError {
    fn from(error: TransportError) -> Self {
        match error.kind() {
            TransportErrorKind::Io => Self::new(BodyErrorKind::Io),
            _ => Self::input(),
        }
    }
}

/// A frame-preserving, safe, dynamically dispatched body.
pub struct DynBody(BoxBody<Bytes, BodyError>);

impl DynBody {
    /// Creates an empty body with an exact zero-byte hint.
    pub fn empty() -> Self {
        Self::from_body(Empty::<Bytes>::new())
    }

    /// Creates a body containing one data frame without copying the bytes.
    pub fn from_bytes(bytes: Bytes) -> Self {
        Self::from_body(Full::new(bytes))
    }

    /// Wraps any `http-body` body and maps its error into the safe body taxonomy.
    pub fn from_body<B>(body: B) -> Self
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Send + 'static,
    {
        let mapped = body.map_err(BodyError::from_producer::<B::Error>);
        Self::box_body(ExclusivePollBody::new(mapped))
    }

    /// Wraps a stream of standard frames without changing frame order or kind.
    pub fn from_frame_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Frame<Bytes>, E>> + Send + 'static,
        E: Send + 'static,
    {
        let stream = FrameResultStream {
            stream: Box::pin(stream),
            _error: PhantomData,
        };
        Self::from_body(UtilStreamBody::new(stream))
    }

    /// Wraps a byte stream, preserving each successful item as one data frame.
    pub fn from_byte_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: Send + 'static,
    {
        let stream = ByteFrameStream {
            stream: Box::pin(stream),
            _error: PhantomData,
        };
        Self::from_body(UtilStreamBody::new(stream))
    }

    /// Wraps an asynchronous reader using the standard 8 KiB chunk size.
    pub fn from_async_read<R>(reader: R) -> Self
    where
        R: AsyncRead + Send + 'static,
    {
        Self::from_async_read_with_chunk_size(reader, 8 * 1024)
            .expect("the standard body chunk size is non-zero")
    }

    /// Wraps an asynchronous reader with a caller-selected nonzero chunk size.
    pub fn from_async_read_with_chunk_size<R>(
        reader: R,
        chunk_size: usize,
    ) -> Result<Self, BodyError>
    where
        R: AsyncRead + Send + 'static,
    {
        let stream = AsyncReadByteStream::new(reader, chunk_size).map_err(BodyError::from)?;
        Ok(Self::from_byte_stream(stream))
    }

    /// Opens a file and supplies its metadata length as an exact size hint.
    pub async fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, BodyError> {
        let file = File::open(path).await.map_err(BodyError::from)?;
        let length = file.metadata().await.map_err(BodyError::from)?.len();
        let body = Self::from_async_read(file);
        Ok(body.with_size_hint(exact_hint(length)))
    }

    /// Converts the legacy stream body while retaining its useful size hint.
    pub fn from_stream_body(body: StreamBody) -> Self {
        let hint = body.size_hint();
        Self::from_byte_stream(body.into_byte_stream()).with_size_hint(hint)
    }

    /// Applies an explicit standard size hint without polling the body.
    pub fn with_size_hint(self, hint: SizeHint) -> Self {
        Self::box_body(HintedBody {
            inner: Box::pin(self),
            lower: hint.lower(),
            upper: hint.upper(),
            terminal: false,
        })
    }

    /// Applies the reusable frame-aware byte limiter.
    pub fn limited(self, limit: u64) -> Self {
        Self::from_body(LimitedBody::new(self, limit))
    }

    /// Structurally enforces an exact delivered length.  This is deliberately
    /// distinct from a `SizeHint`: a hint is advisory while this wrapper
    /// rejects underflow and overflow without yielding excess bytes.
    pub(crate) fn exact_length(self, length: u64) -> Self {
        Self::from_body(ExactLengthBody::new(self, length))
    }

    /// Converts this body to the upstream data-only stream adapter.
    pub fn into_data_stream(self) -> BodyDataStream<Self> {
        BodyExt::into_data_stream(self)
    }

    fn box_body<B>(body: B) -> Self
    where
        B: Body<Data = Bytes, Error = BodyError> + Send + Sync + 'static,
    {
        Self(BoxBody::new(body))
    }
}

/// Applies the common response-body limiter after response-head decisions.
///
/// An upper size hint is safe for an early rejection, but it is only an
/// optimization: `LimitedBody` still counts every data frame that is yielded.
pub(crate) fn limit_response_body(
    body: DynBody,
    limit: Option<usize>,
) -> Result<DynBody, BodyError> {
    let Some(limit) = limit else {
        return Ok(body);
    };
    let limit = u64::try_from(limit).unwrap_or(u64::MAX);
    if let Some(upper) = body.size_hint().upper()
        && upper > limit
    {
        return Err(BodyError::limit_exceeded(limit, upper));
    }
    Ok(body.limited(limit))
}

/// Collects an already-limited frame-aware body. The returned collection keeps
/// trailer frames available until its caller explicitly converts it to bytes.
pub(crate) async fn collect_body(
    body: DynBody,
) -> Result<http_body_util::Collected<Bytes>, BodyError> {
    BodyExt::collect(body).await
}

/// Keeps the resolved attempt origin active for the lifetime of a response
/// body. The lease is deliberately outside the response head and is therefore
/// preserved by every frame-aware body adapter that wraps this body.
pub(crate) fn retain_origin(
    body: DynBody,
    origin: crate::retry_admission::OriginHandle,
) -> DynBody {
    DynBody::from_body(OriginLeasedBody {
        inner: body,
        origin: Some(origin),
    })
}

struct OriginLeasedBody {
    inner: DynBody,
    origin: Option<crate::retry_admission::OriginHandle>,
}

impl Body for OriginLeasedBody {
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        let result = Pin::new(&mut this.inner).poll_frame(cx);
        if matches!(result, Poll::Ready(None) | Poll::Ready(Some(Err(_)))) {
            this.origin.take();
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl fmt::Debug for DynBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynBody")
            .field("size_hint", &self.0.size_hint())
            .field("is_end_stream", &self.0.is_end_stream())
            .finish()
    }
}

impl Body for DynBody {
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.0).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.0.size_hint()
    }
}

/// A frame-aware byte limiter for any standard body using `Bytes` data.
pub struct LimitedBody<B> {
    inner: Pin<Box<B>>,
    limit: u64,
    seen: u64,
    terminal: bool,
}

/// A one-shot structural exact-length guard used by request-body recipes.
///
/// The guard is applied before the legacy `DynBody` transport bridge.  It
/// keeps exact-length contracts meaningful even while the public transport
/// boundary remains `http::Request<DynBody>`.
pub(crate) struct ExactLengthBody<B> {
    inner: Pin<Box<B>>,
    expected: u64,
    seen: u64,
    terminal: bool,
}

impl<B> ExactLengthBody<B> {
    pub(crate) fn new(inner: B, expected: u64) -> Self {
        Self {
            inner: Box::pin(inner),
            expected,
            seen: 0,
            terminal: false,
        }
    }
}

impl<B> Body for ExactLengthBody<B>
where
    B: Body<Data = Bytes, Error = BodyError>,
{
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        if this.terminal {
            return Poll::Ready(None);
        }
        match this.inner.as_mut().poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
                    let observed = this.seen.saturating_add(len);
                    if observed > this.expected {
                        this.terminal = true;
                        return Poll::Ready(Some(Err(BodyError::exact_length_overflow(
                            this.expected,
                            observed,
                        ))));
                    }
                    this.seen = observed;
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => {
                this.terminal = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                this.terminal = true;
                if this.seen == this.expected {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(BodyError::exact_length_underflow(
                        this.expected,
                        this.seen,
                    ))))
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return exact_hint(0);
        }
        exact_hint(self.expected.saturating_sub(self.seen))
    }
}

impl<B> LimitedBody<B> {
    /// Creates a limiter with a byte limit applied only to data frames.
    pub fn new(inner: B, limit: u64) -> Self {
        Self {
            inner: Box::pin(inner),
            limit,
            seen: 0,
            terminal: false,
        }
    }
}

impl<B> fmt::Debug for LimitedBody<B>
where
    B: Body<Data = Bytes, Error = BodyError>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LimitedBody")
            .field("limit", &self.limit)
            .field("seen", &self.seen)
            .field("terminal", &self.terminal)
            .finish()
    }
}

impl<B> Body for LimitedBody<B>
where
    B: Body<Data = Bytes, Error = BodyError>,
{
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        if this.terminal {
            return Poll::Ready(None);
        }

        match this.inner.as_mut().poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let Ok(len) = u64::try_from(data.len()) else {
                        this.terminal = true;
                        return Poll::Ready(Some(Err(BodyError::limit_exceeded(
                            this.limit,
                            u64::MAX,
                        ))));
                    };
                    let Some(observed) = this.seen.checked_add(len) else {
                        this.terminal = true;
                        return Poll::Ready(Some(Err(BodyError::limit_exceeded(
                            this.limit,
                            u64::MAX,
                        ))));
                    };
                    if observed > this.limit {
                        this.terminal = true;
                        return Poll::Ready(Some(Err(BodyError::limit_exceeded(
                            this.limit, observed,
                        ))));
                    }
                    this.seen = observed;
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => {
                this.terminal = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                this.terminal = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal || self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return exact_hint(0);
        }

        let remaining = match self.limit.checked_sub(self.seen) {
            Some(remaining) => remaining,
            None => {
                debug_assert!(self.seen > self.limit);
                0
            }
        };
        let inner = self.inner.size_hint();
        let mut hint = SizeHint::new();
        let lower = inner.lower();
        hint.set_lower(if lower <= remaining { lower } else { 0 });
        let upper = match inner.upper() {
            Some(upper) => upper.min(remaining),
            None => remaining,
        };
        hint.set_upper(upper);
        hint
    }
}

struct ExclusivePollBody<B> {
    inner: Mutex<Pin<Box<B>>>,
    terminal: AtomicBool,
}

impl<B> ExclusivePollBody<B> {
    fn new(inner: B) -> Self {
        Self {
            inner: Mutex::new(Box::pin(inner)),
            terminal: AtomicBool::new(false),
        }
    }
}

impl<B> Body for ExclusivePollBody<B>
where
    B: Body<Data = Bytes, Error = BodyError> + Send + 'static,
{
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_ref().get_ref();
        let mut inner = match this.inner.lock() {
            Ok(inner) => inner,
            Err(_) => {
                if this.terminal.swap(true, Ordering::SeqCst) {
                    return Poll::Ready(None);
                }
                return Poll::Ready(Some(Err(BodyError::exclusive_poll())));
            }
        };
        inner.as_mut().poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        if self.terminal.load(Ordering::SeqCst) {
            return true;
        }
        match self.inner.lock() {
            Ok(inner) => inner.is_end_stream(),
            Err(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal.load(Ordering::SeqCst) {
            return exact_hint(0);
        }
        match self.inner.lock() {
            Ok(inner) => inner.size_hint(),
            Err(_) => SizeHint::new(),
        }
    }
}

struct FrameResultStream<S, E> {
    stream: Pin<Box<S>>,
    _error: PhantomData<fn(E)>,
}

impl<S, E> Stream for FrameResultStream<S, E>
where
    S: Stream<Item = Result<Frame<Bytes>, E>>,
    E: Send + 'static,
{
    type Item = Result<Frame<Bytes>, BodyError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.stream
            .as_mut()
            .poll_next(cx)
            .map(|item| item.map(|result| result.map_err(BodyError::from_producer::<E>)))
    }
}

struct ByteFrameStream<S, E> {
    stream: Pin<Box<S>>,
    _error: PhantomData<fn(E)>,
}

impl<S, E> Stream for ByteFrameStream<S, E>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Send + 'static,
{
    type Item = Result<Frame<Bytes>, BodyError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.stream.as_mut().poll_next(cx).map(|item| {
            item.map(|result| {
                result
                    .map(Frame::data)
                    .map_err(BodyError::from_producer::<E>)
            })
        })
    }
}

struct HintedBody<B> {
    inner: Pin<Box<B>>,
    lower: u64,
    upper: Option<u64>,
    terminal: bool,
}

impl<B> Body for HintedBody<B>
where
    B: Body<Data = Bytes, Error = BodyError>,
{
    type Data = Bytes;
    type Error = BodyError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();
        if this.terminal {
            return Poll::Ready(None);
        }

        match this.inner.as_mut().poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    match u64::try_from(data.len()) {
                        Ok(length) => {
                            this.lower = this.lower.saturating_sub(length);
                            this.upper = match this.upper {
                                Some(upper) if length <= upper => Some(upper - length),
                                Some(_) => None,
                                None => None,
                            };
                        }
                        Err(_) => {
                            this.lower = 0;
                            this.upper = None;
                        }
                    }
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error))),
            Poll::Ready(None) => {
                this.terminal = true;
                this.lower = 0;
                this.upper = Some(0);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal || self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return exact_hint(0);
        }
        let mut hint = SizeHint::new();
        hint.set_lower(self.lower);
        if let Some(upper) = self.upper {
            hint.set_upper(upper);
        }
        hint
    }
}

fn exact_hint(length: u64) -> SizeHint {
    let mut hint = SizeHint::new();
    hint.set_lower(length);
    hint.set_upper(length);
    hint
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, HeaderValue};
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use std::task::Waker;
    use tokio::io::ReadBuf;

    fn poll_with_waker<T>(poll: impl FnOnce(&mut Context<'_>) -> T) -> T {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        poll(&mut cx)
    }

    fn poll_frame(body: &mut DynBody) -> Poll<Option<Result<Frame<Bytes>, BodyError>>> {
        poll_with_waker(|cx| Body::poll_frame(Pin::new(body), cx))
    }

    fn poll_limited<B>(body: &mut LimitedBody<B>) -> Poll<Option<Result<Frame<Bytes>, BodyError>>>
    where
        B: Body<Data = Bytes, Error = BodyError>,
    {
        poll_with_waker(|cx| Body::poll_frame(Pin::new(body), cx))
    }

    fn poll_exact<B>(body: &mut ExactLengthBody<B>) -> Poll<Option<Result<Frame<Bytes>, BodyError>>>
    where
        B: Body<Data = Bytes, Error = BodyError>,
    {
        poll_with_waker(|cx| Body::poll_frame(Pin::new(body), cx))
    }

    #[test]
    fn empty_and_full_bodies_are_exact_and_safe() {
        let empty = DynBody::empty();
        assert_eq!(empty.size_hint().lower(), 0);
        assert_eq!(empty.size_hint().upper(), Some(0));
        assert!(empty.is_end_stream());

        let sentinel = Bytes::from_static(b"BODY_BYTES_MUST_NOT_APPEAR");
        let full = DynBody::from_bytes(sentinel.clone());
        assert_eq!(full.size_hint().lower(), sentinel.len() as u64);
        assert_eq!(full.size_hint().upper(), Some(sentinel.len() as u64));
        assert!(!format!("{full:?}").contains("BODY_BYTES_MUST_NOT_APPEAR"));
        match poll_frame(&mut DynBody::from_bytes(sentinel.clone())) {
            Poll::Ready(Some(Ok(frame))) => assert_eq!(frame.into_data().unwrap(), sentinel),
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn body_and_frame_stream_preserve_frames_and_trailers() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-trailer", HeaderValue::from_static("present"));
        let frames = vec![
            Ok(Frame::data(Bytes::from_static(b"first"))),
            Ok(Frame::trailers(trailers)),
        ];
        let mut body = DynBody::from_frame_stream(OneShotFrames::<BodyError>::new(frames));
        assert!(
            matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(ref frame))) if frame.is_data())
        );
        assert!(
            matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(ref frame))) if frame.is_trailers())
        );
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
    }

    #[test]
    fn direct_body_adapter_preserves_frames_and_trailers() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-direct-trailer", HeaderValue::from_static("present"));
        let mut body = DynBody::from_body(ScriptedBody::new(vec![
            Ok(Frame::data(Bytes::from_static(b"direct-data"))),
            Ok(Frame::trailers(trailers)),
        ]));
        assert!(
            matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(ref frame))) if frame.is_data())
        );
        assert!(matches!(
            poll_frame(&mut body),
            Poll::Ready(Some(Ok(ref frame))) if frame.is_trailers()
        ));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
    }

    #[test]
    fn byte_stream_preserves_boundaries_and_data_stream_discards_trailers() {
        let mut body = DynBody::from_byte_stream(OneShotBytes::<BodyError>::new(vec![
            Ok(Bytes::from_static(b"a")),
            Ok(Bytes::from_static(b"bc")),
        ]));
        match poll_frame(&mut body) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from_static(b"a"))
            }
            other => panic!("unexpected frame: {other:?}"),
        }
        match poll_frame(&mut body) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().unwrap(), Bytes::from_static(b"bc"))
            }
            other => panic!("unexpected frame: {other:?}"),
        }

        let mut stream = Box::pin(
            DynBody::from_frame_stream(OneShotFrames::<BodyError>::new(vec![
                Ok(Frame::data(Bytes::from_static(b"data"))),
                Ok(Frame::trailers(HeaderMap::new())),
            ]))
            .into_data_stream(),
        );
        assert!(
            matches!(poll_with_waker(|cx| Stream::poll_next(stream.as_mut(), cx)), Poll::Ready(Some(Ok(bytes))) if bytes == Bytes::from_static(b"data"))
        );
        assert!(matches!(
            poll_with_waker(|cx| Stream::poll_next(stream.as_mut(), cx)),
            Poll::Ready(None)
        ));
    }

    #[test]
    fn send_only_body_is_boxed_as_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DynBody>();

        let mut body = DynBody::from_body(SendOnlyBody {
            frame: Cell::new(Some(Ok(Frame::data(Bytes::from_static(b"chunk"))))),
        });
        assert!(matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(_)))));
    }

    #[test]
    fn send_only_byte_stream_is_boxed_and_preserves_chunks() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DynBody>();

        let mut body = DynBody::from_byte_stream(SendOnlyBytesStream::new(vec![
            Bytes::from_static(b"one"),
            Bytes::from_static(b"two"),
        ]));
        for expected in [Bytes::from_static(b"one"), Bytes::from_static(b"two")] {
            match poll_frame(&mut body) {
                Poll::Ready(Some(Ok(frame))) => assert_eq!(frame.into_data().unwrap(), expected),
                other => panic!("unexpected frame: {other:?}"),
            }
        }
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
    }

    #[test]
    fn construction_debug_and_hint_are_lazy() {
        let body = DynBody::from_body(PanicBody);
        let _ = body.size_hint();
        let _ = format!("{body:?}");
        drop(body);

        let hinted = DynBody::from_body(PanicBody).with_size_hint(exact_hint(3));
        assert_eq!(hinted.size_hint().exact(), Some(3));
        let _ = format!("{hinted:?}");
        drop(hinted);

        let _ = DynBody::from_frame_stream(PanicFrameStream);
        let _ = DynBody::from_byte_stream(PanicBytesStream);
    }

    #[test]
    fn response_limit_hint_rejects_without_polling() {
        let body = DynBody::from_body(PanicBody).with_size_hint(exact_hint(5));
        let error = limit_response_body(body, Some(4))
            .expect_err("an oversized safe upper hint should fail before polling");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert_eq!(error.limit(), Some(4));
        assert_eq!(error.observed(), Some(5));
    }

    #[tokio::test]
    async fn response_collection_keeps_trailers_until_byte_conversion() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-collected-trailer", HeaderValue::from_static("present"));
        let body = DynBody::from_frame_stream(OneShotFrames::<BodyError>::new(vec![
            Ok(Frame::data(Bytes::from_static(b"data"))),
            Ok(Frame::trailers(trailers)),
        ]));
        let body = limit_response_body(body, Some(4)).expect("body limit");
        let collected = collect_body(body).await.expect("bounded collection");
        assert_eq!(
            collected
                .trailers()
                .and_then(|trailers| trailers.get("x-collected-trailer"))
                .and_then(|value| value.to_str().ok()),
            Some("present")
        );
        assert_eq!(collected.to_bytes(), Bytes::from_static(b"data"));
    }

    #[test]
    fn exact_hinted_body_tracks_each_data_frame_and_completion() {
        let mut body = DynBody::from_body(ScriptedBody::new(vec![
            Ok(Frame::data(Bytes::from_static(b"ab"))),
            Ok(Frame::data(Bytes::from_static(b"cde"))),
            Ok(Frame::trailers(HeaderMap::new())),
        ]))
        .with_size_hint(exact_hint(5));
        assert_eq!(body.size_hint().exact(), Some(5));

        assert!(matches!(
            poll_frame(&mut body),
            Poll::Ready(Some(Ok(ref frame))) if frame.is_data()
        ));
        assert_eq!(body.size_hint().exact(), Some(3));

        assert!(matches!(
            poll_frame(&mut body),
            Poll::Ready(Some(Ok(ref frame))) if frame.is_data()
        ));
        assert_eq!(body.size_hint().exact(), Some(0));

        assert!(matches!(
            poll_frame(&mut body),
            Poll::Ready(Some(Ok(ref frame))) if frame.is_trailers()
        ));
        assert_eq!(body.size_hint().exact(), Some(0));
        assert!(!body.is_end_stream());

        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
        assert!(body.is_end_stream());
        assert_eq!(body.size_hint().exact(), Some(0));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
    }

    #[test]
    fn poisoned_poll_is_reported_once_and_then_terminal() {
        let polls = Arc::new(AtomicUsize::new(0));
        let mut body = DynBody::from_body(PoisonBody {
            polls: Arc::clone(&polls),
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = poll_frame(&mut body);
        }));
        assert!(result.is_err());
        assert_eq!(polls.load(Ordering::SeqCst), 1);

        assert!(matches!(
            poll_frame(&mut body),
            Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::ExclusivePoll
        ));
        assert!(body.is_end_stream());
        assert_eq!(body.size_hint().exact(), Some(0));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn standard_body_preserves_non_exact_hints() {
        let lower_only = DynBody::from_body(ScriptedBody::with_hint(Vec::new(), 3, None));
        assert_eq!(lower_only.size_hint().lower(), 3);
        assert_eq!(lower_only.size_hint().upper(), None);

        let bounded = DynBody::from_body(ScriptedBody::with_hint(Vec::new(), 3, Some(8)));
        assert_eq!(bounded.size_hint().lower(), 3);
        assert_eq!(bounded.size_hint().upper(), Some(8));

        let unknown = DynBody::from_body(ScriptedBody::new(Vec::new()));
        assert_eq!(unknown.size_hint().lower(), 0);
        assert_eq!(unknown.size_hint().upper(), None);
    }

    #[test]
    fn bounded_hinted_body_decrements_lower_and_upper() {
        let mut hint = SizeHint::new();
        hint.set_lower(3);
        hint.set_upper(8);
        let mut body = DynBody::from_body(ScriptedBody::new(vec![Ok(Frame::data(
            Bytes::from_static(b"ab"),
        ))]))
        .with_size_hint(hint);
        assert_eq!(body.size_hint().lower(), 3);
        assert_eq!(body.size_hint().upper(), Some(8));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(_)))));
        assert_eq!(body.size_hint().lower(), 1);
        assert_eq!(body.size_hint().upper(), Some(6));
    }

    #[test]
    fn dishonest_upper_hint_becomes_unknown_after_delivery() {
        let sentinel = Bytes::from_static(b"UPPER_HINT_SENTINEL");
        let mut hint = SizeHint::new();
        hint.set_upper(2);
        let mut body =
            DynBody::from_body(ScriptedBody::new(vec![Ok(Frame::data(sentinel.clone()))]))
                .with_size_hint(hint);
        assert_eq!(body.size_hint().upper(), Some(2));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(_)))));
        assert_eq!(body.size_hint().lower(), 0);
        assert_eq!(body.size_hint().upper(), None);
        assert!(!format!("{body:?}").contains("UPPER_HINT_SENTINEL"));
    }

    #[tokio::test]
    async fn reader_and_file_adapters_preserve_content_and_hints() {
        let reader = DynBody::from_async_read_with_chunk_size(
            std::io::Cursor::new(b"reader-content".to_vec()),
            4,
        )
        .expect("valid reader adapter");
        assert_eq!(reader.size_hint().upper(), None);
        let reader_bytes = reader.collect().await.expect("reader body").to_bytes();
        assert_eq!(reader_bytes, Bytes::from_static(b"reader-content"));

        let path = std::env::temp_dir().join(format!(
            "concord_dyn_body_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        let contents = b"file-content";
        std::fs::write(&path, contents).expect("write body fixture");
        let file = DynBody::from_file(&path).await.expect("file adapter");
        assert_eq!(file.size_hint().exact(), Some(contents.len() as u64));
        let file_bytes = file.collect().await.expect("file body").to_bytes();
        assert_eq!(file_bytes, Bytes::from_static(contents));
        std::fs::remove_file(path).expect("remove body fixture");
    }

    #[tokio::test]
    async fn file_hint_decreases_during_frame_progression() {
        let path = std::env::temp_dir().join(format!(
            "concord_dyn_body_progress_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        ));
        let contents = vec![b'f'; 8 * 1024 + 17];
        std::fs::write(&path, &contents).expect("write progression fixture");
        let mut body = DynBody::from_file(&path).await.expect("file adapter");
        let mut remaining = contents.len() as u64;
        assert_eq!(body.size_hint().exact(), Some(remaining));

        loop {
            match poll_frame(&mut body) {
                Poll::Ready(Some(Ok(frame))) => {
                    if let Some(data) = frame.data_ref() {
                        remaining -= data.len() as u64;
                        assert_eq!(body.size_hint().exact(), Some(remaining));
                    }
                }
                Poll::Ready(None) => break,
                Poll::Ready(Some(Err(error))) => panic!("unexpected file error: {error:?}"),
                Poll::Pending => tokio::task::yield_now().await,
            }
        }
        assert_eq!(remaining, 0);
        assert!(body.is_end_stream());
        assert_eq!(body.size_hint().exact(), Some(0));
        std::fs::remove_file(path).expect("remove progression fixture");
    }

    #[test]
    fn stream_body_hint_decreases_during_progression() {
        let mut hint = SizeHint::new();
        hint.set_lower(3);
        hint.set_upper(8);
        let stream = StreamBody::from_byte_stream(OneShotBytes::<
            crate::stream_body::StreamBodyError,
        >::new(vec![
            Ok(Bytes::from_static(b"ab")),
            Ok(Bytes::from_static(b"cdef")),
        ]))
        .with_size_hint(hint);
        let mut body = DynBody::from_stream_body(stream);
        assert_eq!(body.size_hint().lower(), 3);
        assert_eq!(body.size_hint().upper(), Some(8));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(_)))));
        assert_eq!(body.size_hint().lower(), 1);
        assert_eq!(body.size_hint().upper(), Some(6));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(_)))));
        assert_eq!(body.size_hint().lower(), 0);
        assert_eq!(body.size_hint().upper(), Some(2));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
        assert_eq!(body.size_hint().exact(), Some(0));
    }

    #[test]
    fn byte_stream_errors_remain_typed() {
        let mut body = DynBody::from_byte_stream(OneShotBytes::<BodyError>::new(vec![
            Ok(Bytes::from_static(b"before-error")),
            Err(BodyError::input()),
        ]));
        assert!(matches!(poll_frame(&mut body), Poll::Ready(Some(Ok(_)))));
        assert!(
            matches!(poll_frame(&mut body), Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::Input)
        );
    }

    #[test]
    fn body_errors_are_redacted() {
        let sentinel = "BODY_ERROR_SENTINEL_MUST_NOT_APPEAR";
        let error = BodyError::from(std::io::Error::other(sentinel));
        assert_eq!(error.kind(), BodyErrorKind::Io);
        assert!(!format!("{error:?}").contains(sentinel));
        assert!(!error.to_string().contains(sentinel));
        assert!(error.source().is_none());
    }

    #[test]
    fn limiter_counts_data_not_trailers_and_is_terminal_after_overflow() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-trailer", HeaderValue::from_static("ignored"));
        let inner = ScriptedBody::new(vec![
            Ok(Frame::data(Bytes::from_static(b"ab"))),
            Ok(Frame::trailers(trailers)),
            Ok(Frame::data(Bytes::from_static(b"c"))),
        ]);
        let mut limited = LimitedBody::new(inner, 2);
        assert!(
            matches!(poll_limited(&mut limited), Poll::Ready(Some(Ok(ref frame))) if frame.is_data())
        );
        assert!(
            matches!(poll_limited(&mut limited), Poll::Ready(Some(Ok(ref frame))) if frame.is_trailers())
        );
        assert!(
            matches!(poll_limited(&mut limited), Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::LimitExceeded)
        );
        assert!(matches!(poll_limited(&mut limited), Poll::Ready(None)));
    }

    #[test]
    fn limiter_exact_boundary_and_misleading_hint_are_safe() {
        let inner = ScriptedBody::with_hint(
            vec![Ok(Frame::data(Bytes::from_static(b"abc")))],
            100,
            Some(100),
        );
        let mut limited = LimitedBody::new(inner, 3);
        assert_eq!(limited.size_hint().upper(), Some(3));
        assert!(matches!(
            poll_limited(&mut limited),
            Poll::Ready(Some(Ok(_)))
        ));
        assert_eq!(limited.size_hint().exact(), Some(0));
        assert!(matches!(poll_limited(&mut limited), Poll::Ready(None)));
    }

    #[test]
    fn limiter_exact_boundary_passes_trailers_with_zero_remaining_hint() {
        let mut trailers = HeaderMap::new();
        trailers.insert("x-boundary-trailer", HeaderValue::from_static("present"));
        let mut limited = LimitedBody::new(
            ScriptedBody::with_hint(
                vec![
                    Ok(Frame::data(Bytes::from_static(b"abc"))),
                    Ok(Frame::trailers(trailers)),
                ],
                3,
                Some(3),
            ),
            3,
        );
        assert!(matches!(
            poll_limited(&mut limited),
            Poll::Ready(Some(Ok(ref frame))) if frame.is_data()
        ));
        assert_eq!(limited.size_hint().exact(), Some(0));
        assert!(matches!(
            poll_limited(&mut limited),
            Poll::Ready(Some(Ok(ref frame))) if frame.is_trailers()
        ));
        assert!(matches!(poll_limited(&mut limited), Poll::Ready(None)));
    }

    #[test]
    fn limiter_producer_error_is_not_reclassified() {
        let mut limited = LimitedBody::new(ScriptedBody::new(vec![Err(BodyError::input())]), 100);
        assert!(
            matches!(poll_limited(&mut limited), Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::Input)
        );
    }

    #[test]
    fn limiter_zero_byte_boundary_handles_empty_and_one_byte_bodies() {
        let mut empty = LimitedBody::new(ScriptedBody::new(Vec::new()), 0);
        assert!(matches!(poll_limited(&mut empty), Poll::Ready(None)));

        let mut one = LimitedBody::new(
            ScriptedBody::new(vec![Ok(Frame::data(Bytes::from_static(b"x")))]),
            0,
        );
        assert!(matches!(
            poll_limited(&mut one),
            Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::LimitExceeded
        ));
        assert!(matches!(poll_limited(&mut one), Poll::Ready(None)));
    }

    #[test]
    fn exact_length_guard_rejects_premature_eof_and_never_yields_excess() {
        let mut underflow =
            ExactLengthBody::new(DynBody::from_bytes(Bytes::from_static(b"abc")), 4);
        assert!(matches!(
            poll_exact(&mut underflow),
            Poll::Ready(Some(Ok(_)))
        ));
        assert!(matches!(
            poll_exact(&mut underflow),
            Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::ExactLengthUnderflow
        ));

        let mut overflow =
            ExactLengthBody::new(DynBody::from_bytes(Bytes::from_static(b"abcd")), 3);
        assert!(matches!(
            poll_exact(&mut overflow),
            Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::ExactLengthOverflow
        ));
        assert!(matches!(poll_exact(&mut overflow), Poll::Ready(None)));
    }

    #[test]
    fn exact_length_lifecycle_covers_multiframe_success_and_all_terminal_failures() {
        let mut zero = ExactLengthBody::new(ScriptedBody::new(Vec::new()), 0);
        assert!(matches!(poll_exact(&mut zero), Poll::Ready(None)));

        let mut success = ExactLengthBody::new(
            ScriptedBody::new(vec![
                Ok(Frame::data(Bytes::from_static(b"ab"))),
                Ok(Frame::data(Bytes::from_static(b"cde"))),
            ]),
            5,
        );
        for expected in [Bytes::from_static(b"ab"), Bytes::from_static(b"cde")] {
            match poll_exact(&mut success) {
                Poll::Ready(Some(Ok(frame))) => {
                    assert_eq!(frame.into_data().expect("data"), expected)
                }
                other => panic!("unexpected exact-length success result: {other:?}"),
            }
        }
        assert!(matches!(poll_exact(&mut success), Poll::Ready(None)));

        let mut underflow = ExactLengthBody::new(
            ScriptedBody::new(vec![Ok(Frame::data(Bytes::from_static(b"ab")))]),
            3,
        );
        assert!(matches!(
            poll_exact(&mut underflow),
            Poll::Ready(Some(Ok(_)))
        ));
        assert!(matches!(
            poll_exact(&mut underflow),
            Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::ExactLengthUnderflow
        ));
        assert!(matches!(poll_exact(&mut underflow), Poll::Ready(None)));

        let mut overflow = ExactLengthBody::new(
            ScriptedBody::new(vec![
                Ok(Frame::data(Bytes::from_static(b"ab"))),
                Ok(Frame::data(Bytes::from_static(b"cd"))),
            ]),
            3,
        );
        match poll_exact(&mut overflow) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(frame.into_data().expect("data"), Bytes::from_static(b"ab"))
            }
            other => panic!("unexpected first overflow result: {other:?}"),
        }
        assert!(matches!(
            poll_exact(&mut overflow),
            Poll::Ready(Some(Err(error))) if error.kind() == BodyErrorKind::ExactLengthOverflow
        ));
        assert!(matches!(poll_exact(&mut overflow), Poll::Ready(None)));

        for (items, expected) in [
            (vec![Err(BodyError::input())], BodyErrorKind::Input),
            (
                vec![
                    Ok(Frame::data(Bytes::from_static(b"abc"))),
                    Err(BodyError::invalid_configuration()),
                ],
                BodyErrorKind::InvalidConfiguration,
            ),
        ] {
            let mut body = ExactLengthBody::new(ScriptedBody::new(items), 3);
            if expected == BodyErrorKind::InvalidConfiguration {
                assert!(matches!(poll_exact(&mut body), Poll::Ready(Some(Ok(_)))));
            }
            assert!(matches!(
                poll_exact(&mut body),
                Poll::Ready(Some(Err(error))) if error.kind() == expected
            ));
            assert!(matches!(poll_exact(&mut body), Poll::Ready(None)));
        }
    }

    #[test]
    fn exact_length_cancellation_drops_upstream_before_and_after_partial_delivery() {
        for poll_once in [false, true] {
            let dropped = Arc::new(AtomicBool::new(false));
            let inner = DropCountingBody {
                inner: ScriptedBody::new(vec![Ok(Frame::data(Bytes::from_static(b"partial")))]),
                dropped: Arc::clone(&dropped),
            };
            let mut body = ExactLengthBody::new(inner, 7);
            if poll_once {
                assert!(matches!(poll_exact(&mut body), Poll::Ready(Some(Ok(_)))));
            }
            drop(body);
            assert!(dropped.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn dropping_body_drops_source_without_polling() {
        let dropped = Arc::new(AtomicBool::new(false));
        let source = DropSource {
            dropped: Arc::clone(&dropped),
        };
        drop(DynBody::from_byte_stream(source));
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn dropping_limiter_drops_source_without_draining() {
        let dropped = Arc::new(AtomicBool::new(false));
        let source = DropSource {
            dropped: Arc::clone(&dropped),
        };
        let body = LimitedBody::new(DynBody::from_byte_stream(source), 10);
        drop(body);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn pending_body_is_polled_once_then_dropped() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let source = PendingDropSource {
            polls: Arc::clone(&polls),
            dropped: Arc::clone(&dropped),
        };
        let mut body = DynBody::from_byte_stream(source);
        assert!(matches!(poll_frame(&mut body), Poll::Pending));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        drop(body);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn reader_midstream_io_error_is_typed_and_redacted() {
        let sentinel = "READER_ERROR_SENTINEL_MUST_NOT_APPEAR";
        let mut body =
            DynBody::from_async_read_with_chunk_size(ErrorAfterReader::new(sentinel), 64)
                .expect("valid reader adapter");
        match poll_frame(&mut body) {
            Poll::Ready(Some(Ok(frame))) => {
                assert_eq!(
                    frame.into_data().unwrap(),
                    Bytes::from_static(b"before-error")
                );
            }
            other => panic!("unexpected first reader result: {other:?}"),
        }
        let error = match poll_frame(&mut body) {
            Poll::Ready(Some(Err(error))) => error,
            other => panic!("unexpected reader error result: {other:?}"),
        };
        assert_eq!(error.kind(), BodyErrorKind::Io);
        assert!(!error.to_string().contains(sentinel));
        assert!(!format!("{error:?}").contains(sentinel));
        assert!(error.source().is_none());
        assert!(matches!(poll_frame(&mut body), Poll::Ready(None)));
    }

    struct OneShotFrames<E> {
        items: VecDeque<Result<Frame<Bytes>, E>>,
    }

    impl<E> OneShotFrames<E> {
        fn new(items: Vec<Result<Frame<Bytes>, E>>) -> Self {
            Self {
                items: items.into(),
            }
        }
    }

    impl<E: Unpin> Stream for OneShotFrames<E> {
        type Item = Result<Frame<Bytes>, E>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.get_mut().items.pop_front())
        }
    }

    struct OneShotBytes<E> {
        items: VecDeque<Result<Bytes, E>>,
    }

    impl<E> OneShotBytes<E> {
        fn new(items: Vec<Result<Bytes, E>>) -> Self {
            Self {
                items: items.into(),
            }
        }
    }

    impl<E: Unpin> Stream for OneShotBytes<E> {
        type Item = Result<Bytes, E>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.get_mut().items.pop_front())
        }
    }

    struct SendOnlyBody {
        frame: Cell<Option<Result<Frame<Bytes>, BodyError>>>,
    }

    impl Body for SendOnlyBody {
        type Data = Bytes;
        type Error = BodyError;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(self.frame.take())
        }
    }

    struct PanicBody;

    impl Body for PanicBody {
        type Data = Bytes;
        type Error = BodyError;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            panic!("polling this body is forbidden during construction")
        }
    }

    struct PanicFrameStream;

    impl Stream for PanicFrameStream {
        type Item = Result<Frame<Bytes>, BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            panic!("polling this frame stream is forbidden during construction")
        }
    }

    struct PanicBytesStream;

    impl Stream for PanicBytesStream {
        type Item = Result<Bytes, BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            panic!("polling this byte stream is forbidden during construction")
        }
    }

    struct PoisonBody {
        polls: Arc<AtomicUsize>,
    }

    impl Body for PoisonBody {
        type Data = Bytes;
        type Error = BodyError;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            panic!("poison this body mutex during poll")
        }
    }

    struct SendOnlyBytesStream {
        items: Cell<VecDeque<Bytes>>,
    }

    impl SendOnlyBytesStream {
        fn new(items: Vec<Bytes>) -> Self {
            Self {
                items: Cell::new(items.into()),
            }
        }
    }

    impl Stream for SendOnlyBytesStream {
        type Item = Result<Bytes, BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();
            let mut items = this.items.take();
            let next = items.pop_front().map(Ok);
            this.items.set(items);
            Poll::Ready(next)
        }
    }

    struct ScriptedBody {
        items: VecDeque<Result<Frame<Bytes>, BodyError>>,
        lower: u64,
        upper: Option<u64>,
    }

    struct DropCountingBody {
        inner: ScriptedBody,
        dropped: Arc<AtomicBool>,
    }

    impl Drop for DropCountingBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl Body for DropCountingBody {
        type Data = Bytes;
        type Error = BodyError;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Pin::new(&mut self.inner).poll_frame(cx)
        }
    }

    impl ScriptedBody {
        fn new(items: Vec<Result<Frame<Bytes>, BodyError>>) -> Self {
            Self::with_hint(items, 0, None)
        }

        fn with_hint(
            items: Vec<Result<Frame<Bytes>, BodyError>>,
            lower: u64,
            upper: Option<u64>,
        ) -> Self {
            Self {
                items: items.into(),
                lower,
                upper,
            }
        }
    }

    impl Body for ScriptedBody {
        type Data = Bytes;
        type Error = BodyError;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(self.items.pop_front())
        }

        fn size_hint(&self) -> SizeHint {
            let mut hint = SizeHint::new();
            hint.set_lower(self.lower);
            if let Some(upper) = self.upper {
                hint.set_upper(upper);
            }
            hint
        }
    }

    struct DropSource {
        dropped: Arc<AtomicBool>,
    }

    impl Drop for DropSource {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl Stream for DropSource {
        type Item = Result<Bytes, BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    struct PendingDropSource {
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    impl Drop for PendingDropSource {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl Stream for PendingDropSource {
        type Item = Result<Bytes, BodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Pending
        }
    }

    struct ErrorAfterReader {
        phase: Cell<u8>,
        message: String,
    }

    impl ErrorAfterReader {
        fn new(message: &str) -> Self {
            Self {
                phase: Cell::new(0),
                message: message.to_owned(),
            }
        }
    }

    impl AsyncRead for ErrorAfterReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.phase.get() == 0 {
                self.phase.set(1);
                buffer.put_slice(b"before-error");
                Poll::Ready(Ok(()))
            } else {
                self.phase.set(2);
                Poll::Ready(Err(io::Error::other(self.message.clone())))
            }
        }
    }
}
