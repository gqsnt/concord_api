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

/// An owned one-shot request byte stream.
///
/// Construction is lazy: Concord does not poll the source until native
/// request execution begins. Producer failures are reduced to a structural
/// [`BodyErrorKind::Producer`] and producer-authored text is discarded.
pub struct StreamBody {
    source: BodyStream,
    source_length: Option<u64>,
    declared_length: Option<u64>,
}

impl StreamBody {
    /// Creates a lazy one-frame stream from bytes.
    #[must_use]
    pub fn from_bytes(bytes: Bytes) -> Self {
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        Self {
            source: Box::pin(OneChunk { bytes: Some(bytes) }),
            source_length: Some(length),
            declared_length: None,
        }
    }

    /// Wraps an application stream and sanitizes every producer failure.
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
        }
    }

    /// Creates a lazy byte stream over an asynchronous reader.
    #[must_use]
    pub fn from_reader<R>(reader: R) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
    {
        Self {
            source: Box::pin(ReaderStream { reader }),
            source_length: None,
            declared_length: None,
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
            source: Box::pin(ReaderStream { reader: file }),
            source_length: Some(length.len()),
            declared_length: None,
        })
    }

    /// Declares the exact number of bytes this stream must produce.
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

impl fmt::Debug for StreamBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StreamBody")
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
}

impl<R> Stream for ReaderStream<R>
where
    R: AsyncRead + Unpin,
{
    type Item = Result<Bytes, BodyError>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buffer = vec![0_u8; 8 * 1024];
        let mut read = ReadBuf::new(&mut buffer);
        match Pin::new(&mut self.reader).poll_read(context, &mut read) {
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
                    buffer.truncate(length);
                    Poll::Ready(Some(Ok(Bytes::from(buffer))))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct PollProbe {
        polls: Arc<AtomicUsize>,
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
        let body = StreamBody::from_stream(PollProbe {
            polls: Arc::clone(&polls),
        });
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert_eq!(body.exact_length_value(), None);

        let body = StreamBody::from_reader(&b"reader bytes"[..]).exact_length(12);
        assert_eq!(body.exact_length_value(), Some(12));
        assert_eq!(body.lower_bound(), None);
    }

    #[test]
    fn bytes_have_trustworthy_length_and_conflicts_fail_without_polling() {
        let body = StreamBody::from_bytes(Bytes::from_static(b"four"));
        assert_eq!(body.exact_length_value(), Some(4));
        assert_eq!(body.lower_bound(), Some(4));
        body.validate_metadata().expect("matching source metadata");

        let error = StreamBody::from_bytes(Bytes::from_static(b"four"))
            .exact_length(3)
            .validate_metadata()
            .expect_err("conflicting known source length");
        assert_eq!(error.kind(), BodyErrorKind::InvalidLength);
    }
}
