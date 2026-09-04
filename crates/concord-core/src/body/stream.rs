use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use tokio::io::{AsyncRead, ReadBuf};

use super::{BodyError, BodyErrorKind};

pub(crate) type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, BodyError>> + Send + 'static>>;

const READER_BUFFER_BYTES: usize = 8 * 1024;

/// An owned one-shot request stream.
///
/// Construction is lazy: Concord does not poll the source until native
/// request execution begins. Producer failures are reduced to a structural
/// [`BodyErrorKind::Producer`] and producer-authored text is discarded. The
/// final managed stream suppresses empty chunks before native transport,
/// accepts at most [`MAX_EMPTY_REQUEST_CHUNKS`](super::MAX_EMPTY_REQUEST_CHUNKS)
/// for one request body, and yields cooperatively during immediately ready
/// empty runs. Empty chunks consume neither the body-byte limit nor an exact
/// declared length.
pub struct RequestStream<T = Bytes> {
    source: BodyStream,
    source_length: Option<u64>,
    declared_length: Option<u64>,
    marker: PhantomData<fn() -> T>,
}

impl RequestStream<Bytes> {
    /// Creates a lazy one-frame stream from bytes.
    #[must_use]
    pub fn from_bytes(bytes: Bytes) -> Self {
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        Self {
            source: Box::pin(OneChunk { bytes: Some(bytes) }),
            source_length: Some(length),
            declared_length: None,
            marker: PhantomData,
        }
    }

    /// Wraps an application stream and sanitizes every producer failure.
    ///
    /// The source may return `Pending` and perform trusted application work.
    /// Empty chunks are not a progress mechanism: Concord suppresses and
    /// bounds them when the resulting body enters native transport.
    #[must_use]
    pub fn from_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            source: Box::pin(SanitizedStream::<S, E> {
                inner: Box::pin(stream),
                marker: PhantomData,
            }),
            source_length: None,
            declared_length: None,
            marker: PhantomData,
        }
    }

    /// Creates a lazy byte stream over an asynchronous reader.
    #[must_use]
    pub fn from_reader<R>(reader: R) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        Self {
            source: Box::pin(ReaderStream::new(reader)),
            source_length: None,
            declared_length: None,
            marker: PhantomData,
        }
    }

    /// Opens a file and creates a lazy stream with its current exact length.
    pub async fn from_file(path: impl AsRef<Path>) -> Result<Self, BodyError> {
        let file = tokio::fs::File::open(path.as_ref())
            .await
            .map_err(|error| {
                drop(error);
                BodyError::new(BodyErrorKind::Producer)
            })?;
        let length = file.metadata().await.map_err(|error| {
            drop(error);
            BodyError::new(BodyErrorKind::Producer)
        })?;
        Ok(Self {
            source: Box::pin(ReaderStream::new(file)),
            source_length: Some(length.len()),
            declared_length: None,
            marker: PhantomData,
        })
    }

    /// Declares the exact number of nonempty bytes this stream must produce.
    ///
    /// Empty chunks do not advance this length. Their first total-count excess
    /// fails before any later end-of-stream length validation.
    #[must_use]
    pub fn exact_length(mut self, bytes: u64) -> Self {
        self.declared_length = Some(bytes);
        self
    }

    pub(crate) fn exact_length_value(&self) -> Option<u64> {
        self.declared_length.or(self.source_length)
    }

    pub(crate) const fn lower_bound(&self) -> Option<u64> {
        self.source_length
    }

    pub(crate) fn validate_metadata(&self) -> Result<(), BodyError> {
        if self
            .declared_length
            .zip(self.source_length)
            .is_some_and(|(declared, source)| declared != source)
        {
            return Err(BodyError::new(BodyErrorKind::InvalidLength));
        }
        Ok(())
    }

    pub(crate) fn into_source(self) -> BodyStream {
        self.source
    }
}

impl fmt::Debug for RequestStream<Bytes> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestStream")
            .field("source_length", &self.source_length)
            .field("declared_length", &self.declared_length)
            .field("source", &"<request stream omitted>")
            .finish()
    }
}

struct OneChunk {
    bytes: Option<Bytes>,
}

impl Stream for OneChunk {
    type Item = Result<Bytes, BodyError>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.bytes.take().map(Ok))
    }
}

struct SanitizedStream<S, E> {
    inner: Pin<Box<S>>,
    marker: PhantomData<fn() -> E>,
}

impl<S, E> Stream for SanitizedStream<S, E>
where
    S: Stream<Item = Result<Bytes, E>>,
{
    type Item = Result<Bytes, BodyError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_next(context) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(error))) => {
                drop(error);
                Poll::Ready(Some(Err(BodyError::new(BodyErrorKind::Producer))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

struct ReaderStream<R> {
    reader: R,
    buffer: Box<[u8]>,
}

impl<R> ReaderStream<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: vec![0_u8; READER_BUFFER_BYTES].into_boxed_slice(),
        }
    }
}

impl<R> Stream for ReaderStream<R>
where
    R: AsyncRead + Unpin,
{
    type Item = Result<Bytes, BodyError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let mut read = ReadBuf::new(&mut this.buffer);
        match Pin::new(&mut this.reader).poll_read(context, &mut read) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => {
                drop(error);
                Poll::Ready(Some(Err(BodyError::new(BodyErrorKind::Producer))))
            }
            Poll::Ready(Ok(())) => {
                let length = read.filled().len();
                if length == 0 {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(Bytes::copy_from_slice(read.filled()))))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::poll_fn;
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct PollProbe {
        polls: Arc<AtomicUsize>,
    }

    #[derive(Default)]
    struct PendingReader {
        polls: usize,
        buffers: Vec<(usize, usize)>,
    }

    impl AsyncRead for PendingReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
            read: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let buffer = read.initialize_unfilled();
            self.buffers.push((buffer.as_ptr() as usize, buffer.len()));
            self.polls += 1;
            match self.polls {
                1 | 2 => {
                    context.waker().wake_by_ref();
                    Poll::Pending
                }
                3 => {
                    read.put_slice(b"reader bytes");
                    Poll::Ready(Ok(()))
                }
                _ => Poll::Ready(Ok(())),
            }
        }
    }

    impl Stream for PollProbe {
        type Item = Result<Bytes, io::Error>;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(None)
        }
    }

    #[tokio::test]
    async fn constructors_are_lazy_and_reader_is_chunked() {
        let polls = Arc::new(AtomicUsize::new(0));
        let body = RequestStream::from_stream(PollProbe {
            polls: Arc::clone(&polls),
        });
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert_eq!(body.exact_length_value(), None);

        let body = RequestStream::from_reader(&b"reader bytes"[..]).exact_length(12);
        assert_eq!(body.exact_length_value(), Some(12));
        assert_eq!(body.lower_bound(), None);
    }

    #[tokio::test]
    async fn reader_reuses_one_buffer_across_pending_data_and_eof() {
        let mut stream = ReaderStream::new(PendingReader::default());
        let data = poll_fn(|context| Pin::new(&mut stream).poll_next(context))
            .await
            .expect("reader data")
            .expect("successful reader data");
        assert_eq!(data, Bytes::from_static(b"reader bytes"));
        assert!(
            poll_fn(|context| Pin::new(&mut stream).poll_next(context))
                .await
                .is_none()
        );

        assert_eq!(stream.reader.buffers.len(), 4);
        let first = stream.reader.buffers[0];
        assert_eq!(first.1, READER_BUFFER_BYTES);
        assert!(
            stream
                .reader
                .buffers
                .iter()
                .all(|&(pointer, capacity)| pointer == first.0 && capacity == first.1)
        );
    }

    #[test]
    fn bytes_have_trustworthy_length_and_conflicts_fail_without_polling() {
        let body = RequestStream::from_bytes(Bytes::from_static(b"four"));
        assert_eq!(body.exact_length_value(), Some(4));
        assert_eq!(body.lower_bound(), Some(4));
        body.validate_metadata().expect("matching source metadata");

        let error = RequestStream::from_bytes(Bytes::from_static(b"four"))
            .exact_length(3)
            .validate_metadata()
            .expect_err("conflicting known source length");
        assert_eq!(error.kind(), BodyErrorKind::InvalidLength);
    }
}
