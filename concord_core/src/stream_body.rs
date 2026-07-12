use crate::transport::{TransportByteStream, TransportError, TransportErrorKind};
use bytes::Bytes;
use futures_core::Stream;
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::File;
use tokio::io::{AsyncRead, ReadBuf};

const DEFAULT_CHUNK_SIZE: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BodySizeHint {
    lower: u64,
    upper: Option<u64>,
}

impl BodySizeHint {
    pub const fn unknown() -> Self {
        Self {
            lower: 0,
            upper: None,
        }
    }

    pub const fn exact(len: u64) -> Self {
        Self {
            lower: len,
            upper: Some(len),
        }
    }

    pub const fn at_least(lower: u64) -> Self {
        Self { lower, upper: None }
    }

    pub fn between(lower: u64, upper: u64) -> Result<Self, StreamBodyError> {
        if upper < lower {
            return Err(StreamBodyError::size_hint());
        }
        Ok(Self {
            lower,
            upper: Some(upper),
        })
    }

    pub const fn lower(self) -> u64 {
        self.lower
    }

    pub const fn upper(self) -> Option<u64> {
        self.upper
    }

    pub const fn exact_len(self) -> Option<u64> {
        match self.upper {
            Some(upper) if upper == self.lower => Some(upper),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamBodyErrorKind {
    Io,
    InvalidChunkSize,
    SizeHint,
    Transport,
}

pub struct StreamBodyError {
    kind: StreamBodyErrorKind,
}

impl StreamBodyError {
    pub fn io(_error: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind: StreamBodyErrorKind::Io,
        }
    }

    pub fn invalid_chunk_size() -> Self {
        Self {
            kind: StreamBodyErrorKind::InvalidChunkSize,
        }
    }

    pub fn size_hint() -> Self {
        Self {
            kind: StreamBodyErrorKind::SizeHint,
        }
    }

    pub fn transport(_error: impl Error + Send + Sync + 'static) -> Self {
        Self {
            kind: StreamBodyErrorKind::Transport,
        }
    }

    pub fn kind(&self) -> StreamBodyErrorKind {
        self.kind
    }
}

impl fmt::Debug for StreamBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamBodyError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for StreamBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind {
            StreamBodyErrorKind::Io => "stream body I/O error",
            StreamBodyErrorKind::InvalidChunkSize => "stream body chunk size must be non-zero",
            StreamBodyErrorKind::SizeHint => "stream body size hint is invalid",
            StreamBodyErrorKind::Transport => "stream body transport error",
        })
    }
}

impl Error for StreamBodyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl From<std::io::Error> for StreamBodyError {
    fn from(error: std::io::Error) -> Self {
        Self::io(error)
    }
}

impl From<StreamBodyError> for TransportError {
    fn from(error: StreamBodyError) -> Self {
        let kind = match error.kind {
            StreamBodyErrorKind::Io => TransportErrorKind::Io,
            StreamBodyErrorKind::InvalidChunkSize | StreamBodyErrorKind::SizeHint => {
                TransportErrorKind::Request
            }
            StreamBodyErrorKind::Transport => TransportErrorKind::Other,
        };
        TransportError::with_kind(kind, error)
    }
}

pub struct StreamBody {
    stream: TransportByteStream,
    size_hint: BodySizeHint,
}

impl StreamBody {
    pub fn from_bytes(bytes: Bytes) -> Self {
        let size_hint = BodySizeHint::exact(bytes.len() as u64);
        Self {
            stream: TransportByteStream::new(OnceBytesStream::new(bytes)),
            size_hint,
        }
    }

    pub fn from_byte_stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, StreamBodyError>> + Send + 'static,
    {
        Self {
            stream: TransportByteStream::new(stream),
            size_hint: BodySizeHint::unknown(),
        }
    }

    pub fn from_async_read<R>(reader: R) -> Self
    where
        R: AsyncRead + Send + 'static,
    {
        Self::from_async_read_with_chunk_size(reader, DEFAULT_CHUNK_SIZE)
            .expect("default chunk size is non-zero")
    }

    pub fn from_async_read_with_chunk_size<R>(
        reader: R,
        chunk_size: usize,
    ) -> Result<Self, StreamBodyError>
    where
        R: AsyncRead + Send + 'static,
    {
        Ok(Self {
            stream: TransportByteStream::new(AsyncReadByteStream::new(reader, chunk_size)?),
            size_hint: BodySizeHint::unknown(),
        })
    }

    pub async fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, StreamBodyError> {
        let file = File::open(path).await?;
        let size_hint = BodySizeHint::exact(file.metadata().await?.len());
        Ok(Self {
            stream: TransportByteStream::new(AsyncReadByteStream::new(file, DEFAULT_CHUNK_SIZE)?),
            size_hint,
        })
    }

    pub fn with_size_hint(mut self, hint: BodySizeHint) -> Self {
        self.size_hint = hint;
        self
    }

    pub fn size_hint(&self) -> BodySizeHint {
        self.size_hint
    }

    pub(crate) fn into_transport_stream(self) -> TransportByteStream {
        self.stream
    }
}

impl fmt::Debug for StreamBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamBody")
            .field("size_hint", &self.size_hint)
            .finish_non_exhaustive()
    }
}

struct OnceBytesStream {
    bytes: Option<Bytes>,
}

impl OnceBytesStream {
    fn new(bytes: Bytes) -> Self {
        Self { bytes: Some(bytes) }
    }
}

impl Stream for OnceBytesStream {
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().bytes.take().map(Ok))
    }
}

pub(crate) struct AsyncReadByteStream<R> {
    reader: Pin<Box<R>>,
    buffer: Vec<u8>,
    eof: bool,
}

impl<R> AsyncReadByteStream<R>
where
    R: AsyncRead + Send + 'static,
{
    pub(crate) fn new(reader: R, chunk_size: usize) -> Result<Self, StreamBodyError> {
        if chunk_size == 0 {
            return Err(StreamBodyError::invalid_chunk_size());
        }
        Ok(Self {
            reader: Box::pin(reader),
            buffer: vec![0u8; chunk_size],
            eof: false,
        })
    }
}

impl<R> Stream for AsyncReadByteStream<R>
where
    R: AsyncRead + Send + 'static,
{
    type Item = Result<Bytes, StreamBodyError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.eof {
            return Poll::Ready(None);
        }
        let mut read_buf = ReadBuf::new(&mut this.buffer);
        match this.reader.as_mut().poll_read(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled();
                if filled.is_empty() {
                    this.eof = true;
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(Bytes::copy_from_slice(filled))))
                }
            }
            Poll::Ready(Err(error)) => {
                this.eof = true;
                Poll::Ready(Some(Err(StreamBodyError::io(error))))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportError;
    use std::cell::Cell;
    use std::io;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn poll_next(stream: &mut TransportByteStream) -> Poll<Option<Result<Bytes, TransportError>>> {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        Pin::new(stream).poll_next(&mut cx)
    }

    #[test]
    fn stream_body_from_bytes_yields_chunks_and_hides_payload_in_debug() {
        let sentinel = Bytes::from_static(b"SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR");
        let body = StreamBody::from_bytes(sentinel.clone());
        assert_eq!(body.size_hint().exact_len(), Some(sentinel.len() as u64));
        assert!(!format!("{:?}", body).contains("SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR"));

        let mut stream = body.into_transport_stream();
        match poll_next(&mut stream) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, sentinel),
            other => panic!("unexpected stream poll result: {other:?}"),
        }
        assert!(matches!(poll_next(&mut stream), Poll::Ready(None)));
    }

    #[test]
    fn stream_body_from_byte_stream_preserves_stream_errors_safely() {
        let sentinel = Bytes::from_static(b"SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR");
        let body = StreamBody::from_byte_stream(ErrorStream::new(StreamBodyError::transport(
            std::io::Error::other(String::from_utf8_lossy(&sentinel).to_string()),
        )));
        let mut stream = body.into_transport_stream();
        let error = match poll_next(&mut stream) {
            Poll::Ready(Some(Err(error))) => error,
            other => panic!("unexpected stream poll result: {other:?}"),
        };
        let debug = format!("{:?}", error);
        let display = format!("{error}");
        assert!(!debug.contains("SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR"));
        assert!(!display.contains("SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR"));
        assert!(!error_chain_contains(
            &error,
            "SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR"
        ));
    }

    #[test]
    fn stream_body_accepts_send_not_sync_streams() {
        let body = StreamBody::from_byte_stream(SendOnlyStream::new());
        let mut stream = body.into_transport_stream();
        match poll_next(&mut stream) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, Bytes::from_static(b"chunk")),
            other => panic!("unexpected stream poll result: {other:?}"),
        }
    }

    #[test]
    fn body_size_hint_constructors_are_consistent() {
        let unknown = BodySizeHint::unknown();
        assert_eq!(unknown.lower(), 0);
        assert_eq!(unknown.upper(), None);
        assert_eq!(unknown.exact_len(), None);

        let exact = BodySizeHint::exact(7);
        assert_eq!(exact.lower(), 7);
        assert_eq!(exact.upper(), Some(7));
        assert_eq!(exact.exact_len(), Some(7));

        let at_least = BodySizeHint::at_least(9);
        assert_eq!(at_least.lower(), 9);
        assert_eq!(at_least.upper(), None);
        assert_eq!(at_least.exact_len(), None);

        let between = BodySizeHint::between(3, 8).expect("valid size hint");
        assert_eq!(between.lower(), 3);
        assert_eq!(between.upper(), Some(8));
        assert_eq!(between.exact_len(), None);

        let error = BodySizeHint::between(5, 4).expect_err("invalid range should fail");
        assert_eq!(error.kind(), StreamBodyErrorKind::SizeHint);
    }

    #[test]
    fn async_read_rejects_zero_chunk_size() {
        let error = StreamBody::from_async_read_with_chunk_size(tokio::io::empty(), 0)
            .expect_err("chunk size zero should be rejected");
        assert_eq!(error.kind(), StreamBodyErrorKind::InvalidChunkSize);
    }

    fn error_chain_contains(error: &(dyn std::error::Error + 'static), needle: &str) -> bool {
        let mut current = Some(error);
        while let Some(err) = current {
            if err.to_string().contains(needle) || format!("{err:?}").contains(needle) {
                return true;
            }
            current = err.source();
        }
        false
    }

    #[test]
    fn stream_body_error_source_chain_is_body_free() {
        let sentinel = "SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR";
        let error = StreamBodyError::transport(std::io::Error::other(sentinel));

        assert!(error.source().is_none());
        assert!(!error_chain_contains(&error, sentinel));
    }

    fn patterned_vec(len: usize) -> Vec<u8> {
        (0..len).map(|idx| (idx % 251) as u8).collect()
    }

    async fn drain_stream_body(
        body: StreamBody,
    ) -> Result<(usize, usize, Vec<u8>), TransportError> {
        let mut stream = body.into_transport_stream();
        let mut total_bytes = 0usize;
        let mut total_chunks = 0usize;
        let mut collected = Vec::new();
        loop {
            match poll_next(&mut stream) {
                Poll::Ready(Some(Ok(bytes))) => {
                    total_bytes += bytes.len();
                    total_chunks += 1;
                    collected.extend_from_slice(bytes.as_ref());
                }
                Poll::Ready(Some(Err(error))) => return Err(error),
                Poll::Ready(None) => return Ok((total_bytes, total_chunks, collected)),
                Poll::Pending => tokio::task::yield_now().await,
            }
        }
    }

    #[tokio::test]
    async fn async_read_empty_reader_yields_no_chunks() {
        let body = StreamBody::from_async_read_with_chunk_size(tokio::io::empty(), 4)
            .expect("valid chunk size");
        let (bytes, chunks, collected) = drain_stream_body(body).await.expect("empty reader");

        assert_eq!(bytes, 0);
        assert_eq!(chunks, 0);
        assert!(collected.is_empty());
    }

    #[tokio::test]
    async fn async_read_exact_multiple_yields_expected_chunks_and_bytes() {
        let payload = patterned_vec(16);
        let body = StreamBody::from_async_read_with_chunk_size(io::Cursor::new(payload.clone()), 4)
            .expect("valid chunk size");
        let (bytes, chunks, collected) = drain_stream_body(body).await.expect("drain body");

        assert_eq!(bytes, payload.len());
        assert_eq!(chunks, 4);
        assert!(
            collected == payload,
            "async-read exact multiple payload changed"
        );
    }

    #[tokio::test]
    async fn async_read_partial_final_chunk_yields_expected_bytes() {
        let payload = patterned_vec(10);
        let body = StreamBody::from_async_read_with_chunk_size(io::Cursor::new(payload.clone()), 4)
            .expect("valid chunk size");
        let (bytes, chunks, collected) = drain_stream_body(body).await.expect("drain body");

        assert_eq!(bytes, payload.len());
        assert_eq!(chunks, 3);
        assert!(
            collected == payload,
            "async-read partial final chunk payload changed"
        );
    }

    #[tokio::test]
    async fn async_read_larger_payload_drains_exactly() {
        let payload = patterned_vec(128 * 1024);
        let body =
            StreamBody::from_async_read_with_chunk_size(io::Cursor::new(payload.clone()), 8 * 1024)
                .expect("valid chunk size");
        let (bytes, chunks, collected) = drain_stream_body(body).await.expect("drain body");

        assert_eq!(bytes, payload.len());
        assert_eq!(chunks, 16);
        assert!(collected == payload, "async-read larger payload changed");
    }

    #[tokio::test]
    async fn async_read_error_propagates_without_exposing_body_bytes() {
        let sentinel = "SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR";
        let body = StreamBody::from_async_read_with_chunk_size(ErrorReader::new(sentinel), 8)
            .expect("valid chunk size");
        let error = drain_stream_body(body)
            .await
            .expect_err("reader error should propagate");

        let display = error.to_string();
        let debug = format!("{error:?}");
        assert!(display.contains("transport error"));
        assert!(!display.contains(sentinel));
        assert!(!debug.contains(sentinel));
        assert!(!error_chain_contains(&error, sentinel));
    }

    #[tokio::test]
    async fn stream_body_from_file_streams_contents_and_reports_known_size() {
        let unique = format!(
            "concord_stream_body_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        let sentinel = b"SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR";
        std::fs::write(&path, sentinel).expect("write temp file");

        let body = StreamBody::from_file(&path).await.expect("file body");
        assert_eq!(body.size_hint().exact_len(), Some(sentinel.len() as u64));
        assert!(!format!("{:?}", body).contains("SECRET_STREAM_BODY_SENTINEL_MUST_NOT_APPEAR"));

        let mut stream = body.into_transport_stream();
        let mut collected = Vec::new();
        loop {
            match poll_next(&mut stream) {
                Poll::Ready(Some(Ok(bytes))) => collected.extend_from_slice(bytes.as_ref()),
                Poll::Ready(Some(Err(error))) => panic!("unexpected error: {error:?}"),
                Poll::Ready(None) => break,
                Poll::Pending => tokio::task::yield_now().await,
            }
        }

        assert_eq!(collected, sentinel);
        let _ = std::fs::remove_file(&path);
    }

    struct ErrorStream {
        item: Option<Result<Bytes, StreamBodyError>>,
    }

    impl ErrorStream {
        fn new(item: StreamBodyError) -> Self {
            Self {
                item: Some(Err(item)),
            }
        }
    }

    impl Stream for ErrorStream {
        type Item = Result<Bytes, StreamBodyError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.item.take())
        }
    }

    struct SendOnlyStream {
        chunk: Cell<Option<Result<Bytes, StreamBodyError>>>,
    }

    impl SendOnlyStream {
        fn new() -> Self {
            Self {
                chunk: Cell::new(Some(Ok(Bytes::from_static(b"chunk")))),
            }
        }
    }

    impl Stream for SendOnlyStream {
        type Item = Result<Bytes, StreamBodyError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.chunk.take())
        }
    }

    struct ErrorReader {
        message: String,
    }

    impl ErrorReader {
        fn new(message: &str) -> Self {
            Self {
                message: message.to_owned(),
            }
        }
    }

    impl AsyncRead for ErrorReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other(self.message.clone())))
        }
    }
}
