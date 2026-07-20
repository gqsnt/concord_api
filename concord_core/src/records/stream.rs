use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::BytesMut;
use futures_core::Stream;

use crate::response::{
    ByteStream, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead,
    ResponseMode, Streaming,
};

use super::{RecordDecoder, RecordError, RecordErrorKind};

/// A streaming successful-response specification decoded by `D`.
///
/// It selects Concord's sealed streaming mode and returns a [`RecordStream`]
/// without introducing another executor or acquisition mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Records<D>(PhantomData<fn() -> D>);

impl<D> ResponseBodySpec for Records<D>
where
    D: RecordDecoder,
{
    type Output = RecordStream<D>;
    type Mode = Streaming;

    fn accept() -> Option<&'static str> {
        D::accept()
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        D::validate(head).map_err(response_error)
    }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        let decoder = D::start(head).map_err(response_error)?;
        Ok(RecordStream::new(body, decoder, D::max_buffered_bytes()))
    }
}

fn response_error(error: RecordError) -> ResponseBodyError {
    let kind = match error.kind() {
        RecordErrorKind::InvalidMediaType => ResponseBodyErrorKind::InvalidMediaType,
        RecordErrorKind::Internal => ResponseBodyErrorKind::Internal,
        RecordErrorKind::InvalidFraming
        | RecordErrorKind::Decode
        | RecordErrorKind::IncompleteRecord
        | RecordErrorKind::LimitExceeded
        | RecordErrorKind::Body => ResponseBodyErrorKind::InvalidOutput,
    };
    drop(error);
    ResponseBodyError::new(kind)
}

/// A lazy stream of incrementally framed typed records.
///
/// Staging is bounded independently of the cumulative [`ByteStream`] limit. An
/// append that would first exceed the decoder's bound is rejected without
/// retaining its bytes, and the oversized record is not yielded. Body and
/// decode failures after opening are stream items. Dropping this value drops
/// the underlying byte stream and cancels native body consumption. `Debug`
/// omits the byte stream, decoder state, and staged bytes and never formats
/// records.
pub struct RecordStream<D>
where
    D: RecordDecoder,
{
    body: Option<ByteStream>,
    decoder: D,
    buffer: BytesMut,
    max_buffered_bytes: usize,
    eof: bool,
    done: bool,
}

impl<D> RecordStream<D>
where
    D: RecordDecoder,
{
    pub(crate) fn new(body: ByteStream, decoder: D, max_buffered_bytes: NonZeroUsize) -> Self {
        Self {
            body: Some(body),
            decoder,
            buffer: BytesMut::new(),
            max_buffered_bytes: max_buffered_bytes.get(),
            eof: false,
            done: false,
        }
    }

    /// Groups records into non-empty batches of at most `size` records.
    #[must_use]
    pub fn batches(self, size: NonZeroUsize) -> RecordBatches<D> {
        RecordBatches {
            stream: self,
            size: size.get(),
            pending: Vec::new(),
            done: false,
        }
    }

    fn fail(&mut self, kind: RecordErrorKind) -> Poll<Option<Result<D::Record, RecordError>>> {
        self.done = true;
        self.buffer.clear();
        self.body = None;
        Poll::Ready(Some(Err(RecordError::new(kind))))
    }
}

impl<D> Unpin for RecordStream<D> where D: RecordDecoder {}

impl<D> Stream for RecordStream<D>
where
    D: RecordDecoder,
{
    type Item = Result<D::Record, RecordError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }

        loop {
            let before = this.buffer.len();
            let decoded = if this.eof {
                this.decoder.decode_eof(&mut this.buffer)
            } else {
                this.decoder.decode(&mut this.buffer)
            };
            if this.buffer.len() > this.max_buffered_bytes {
                return this.fail(RecordErrorKind::LimitExceeded);
            }
            match decoded {
                Ok(Some(record)) => return Poll::Ready(Some(Ok(record))),
                Err(error) => {
                    this.done = true;
                    this.buffer.clear();
                    this.body = None;
                    return Poll::Ready(Some(Err(error)));
                }
                Ok(None) if this.eof => {
                    this.done = true;
                    if this.buffer.is_empty() {
                        return Poll::Ready(None);
                    }
                    this.buffer.clear();
                    return Poll::Ready(Some(Err(RecordError::new(
                        RecordErrorKind::IncompleteRecord,
                    ))));
                }
                Ok(None) if this.buffer.len() < before => continue,
                Ok(None) => {}
            }

            let Some(body) = this.body.as_mut() else {
                this.eof = true;
                continue;
            };
            match Pin::new(body).poll_next(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    this.body = None;
                    this.eof = true;
                }
                Poll::Ready(Some(Err(error))) => {
                    drop(error);
                    return this.fail(RecordErrorKind::Body);
                }
                Poll::Ready(Some(Ok(chunk))) => {
                    let Some(staged) = this.buffer.len().checked_add(chunk.len()) else {
                        return this.fail(RecordErrorKind::LimitExceeded);
                    };
                    if staged > this.max_buffered_bytes {
                        return this.fail(RecordErrorKind::LimitExceeded);
                    }
                    this.buffer.extend_from_slice(&chunk);
                }
            }
        }
    }
}

impl<D> fmt::Debug for RecordStream<D>
where
    D: RecordDecoder,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordStream")
            .field("body", &"<byte stream omitted>")
            .field("decoder", &"<record decoder omitted>")
            .field("staging", &"<record bytes omitted>")
            .field("eof", &self.eof)
            .field("done", &self.done)
            .finish()
    }
}

/// A consumer adapter yielding non-empty vectors of decoded records.
///
/// One-record streaming remains the primitive. This adapter holds at most the
/// requested positive batch size and discards a partial batch if the underlying
/// stream fails. Its `Debug` implementation never formats records.
pub struct RecordBatches<D>
where
    D: RecordDecoder,
{
    stream: RecordStream<D>,
    size: usize,
    pending: Vec<D::Record>,
    done: bool,
}

impl<D> Unpin for RecordBatches<D> where D: RecordDecoder {}

impl<D> Stream for RecordBatches<D>
where
    D: RecordDecoder,
{
    type Item = Result<Vec<D::Record>, RecordError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }

        loop {
            match Pin::new(&mut this.stream).poll_next(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(record))) => {
                    this.pending.push(record);
                    if this.pending.len() == this.size {
                        return Poll::Ready(Some(Ok(std::mem::take(&mut this.pending))));
                    }
                }
                Poll::Ready(Some(Err(error))) => {
                    this.done = true;
                    this.pending.clear();
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(None) => {
                    this.done = true;
                    return if this.pending.is_empty() {
                        Poll::Ready(None)
                    } else {
                        Poll::Ready(Some(Ok(std::mem::take(&mut this.pending))))
                    };
                }
            }
        }
    }
}

impl<D> fmt::Debug for RecordBatches<D>
where
    D: RecordDecoder,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordBatches")
            .field("stream", &"<record stream omitted>")
            .field("batch_size", &self.size)
            .field("pending", &"<records omitted>")
            .field("done", &self.done)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::poll_fn;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use bytes::{Bytes, BytesMut};

    use super::*;
    use crate::error::{ApiError, ErrorCategory};
    use crate::http::application::{ApplicationByteStream, bounded_byte_stream};

    struct LineDecoder;

    impl RecordDecoder for LineDecoder {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            Ok(Self)
        }

        fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
                return Ok(None);
            };
            let mut record = buffer.split_to(end + 1);
            record.truncate(end);
            Ok(Some(record.to_vec()))
        }

        fn decode_eof(
            &mut self,
            buffer: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            if buffer.is_empty() {
                Ok(None)
            } else {
                Ok(Some(buffer.split_to(buffer.len()).to_vec()))
            }
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(8).expect("positive test bound")
        }
    }

    struct ProbeBody {
        chunks: VecDeque<Result<Bytes, ApiError>>,
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    impl Stream for ProbeBody {
        type Item = Result<Bytes, ApiError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(self.chunks.pop_front())
        }
    }

    impl Drop for ProbeBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    fn stream(
        chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        staging: usize,
    ) -> RecordStream<LineDecoder> {
        let body: ApplicationByteStream = Box::pin(ProbeBody {
            chunks: chunks.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
        });
        let body = ByteStream::new(bounded_byte_stream(
            body,
            "RecordsApi::custom",
            http::Method::GET,
            1024,
        ));
        RecordStream::new(
            body,
            LineDecoder,
            NonZeroUsize::new(staging).expect("positive test staging bound"),
        )
    }

    async fn next<S>(stream: &mut S) -> Option<S::Item>
    where
        S: Stream + Unpin,
    {
        poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
    }

    #[tokio::test]
    async fn record_boundaries_are_chunk_independent_and_buffered_records_precede_another_poll() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut records = stream(
            [
                Ok(Bytes::from_static(b"ab")),
                Ok(Bytes::from_static(b"c\nx\n")),
            ],
            &polls,
            &dropped,
            8,
        );
        assert_eq!(next(&mut records).await.unwrap().unwrap(), b"abc");
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert_eq!(next(&mut records).await.unwrap().unwrap(), b"x");
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(next(&mut records).await.is_none());
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn first_staging_excess_is_withheld_and_error_formatting_omits_bytes() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut records = stream(
            [Ok(Bytes::from_static(b"oversized-record-sentinel\n"))],
            &polls,
            &dropped,
            4,
        );
        let error = next(&mut records)
            .await
            .unwrap()
            .expect_err("staging excess");
        assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
        assert!(dropped.load(Ordering::SeqCst));
        assert!(next(&mut records).await.is_none());
        assert!(!format!("{error} {error:?}").contains("oversized-record-sentinel"));
    }

    #[tokio::test]
    async fn body_failures_are_safe_stream_items_and_drop_cancels_the_byte_stream() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let body_error = ApiError::classified(
            "RecordsApi::custom",
            http::Method::GET,
            ErrorCategory::ResponseBody,
        );
        let mut records = stream([Err(body_error)], &polls, &dropped, 8);
        let error = next(&mut records)
            .await
            .unwrap()
            .expect_err("body error item");
        assert_eq!(error.kind(), RecordErrorKind::Body);
        assert!(dropped.load(Ordering::SeqCst));

        let drop_polls = Arc::new(AtomicUsize::new(0));
        let drop_observed = Arc::new(AtomicBool::new(false));
        let records = stream(
            [Ok(Bytes::from_static(b"unpolled-record-sentinel\n"))],
            &drop_polls,
            &drop_observed,
            32,
        );
        assert!(!format!("{records:?}").contains("unpolled-record-sentinel"));
        drop(records);
        assert!(drop_observed.load(Ordering::SeqCst));
        assert_eq!(drop_polls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn batching_yields_full_then_final_partial_batches() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut batches = stream([Ok(Bytes::from_static(b"a\nb\nc\n"))], &polls, &dropped, 8)
            .batches(NonZeroUsize::new(2).expect("positive batch"));
        assert_eq!(
            next(&mut batches).await.unwrap().unwrap(),
            [b"a".to_vec(), b"b".to_vec()]
        );
        assert_eq!(next(&mut batches).await.unwrap().unwrap(), [b"c".to_vec()]);
        assert!(next(&mut batches).await.is_none());
    }
}
