use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

use crate::response::{
    ByteStream, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead,
    ResponseMode, Streaming,
};

use super::driver::{RecordDriver, RecordStep};
use super::{RecordDecoder, RecordError, RecordErrorKind};

/// A streaming accepted-response specification decoded by `D`.
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
/// Staging is bounded independently of the cumulative [`ByteStream`] limit.
/// Large source chunks are retained separately and copied into decoder staging
/// only as records make room. A logical record's first excess staging byte is
/// rejected without being retained, and the oversized record is not yielded.
/// Body and decode failures after opening are stream items. Dropping this value
/// drops the underlying byte stream and cancels native body consumption.
/// `Debug` omits the byte stream, decoder state, pending source bytes, and
/// staged bytes and never formats records.
pub struct RecordStream<D>
where
    D: RecordDecoder,
{
    body: Option<ByteStream>,
    driver: RecordDriver<D>,
}

impl<D> RecordStream<D>
where
    D: RecordDecoder,
{
    pub(crate) fn new(body: ByteStream, decoder: D, max_buffered_bytes: NonZeroUsize) -> Self {
        Self {
            body: Some(body),
            driver: RecordDriver::new(decoder, max_buffered_bytes),
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

    fn fail(&mut self, error: RecordError) -> Poll<Option<Result<D::Record, RecordError>>> {
        self.body = None;
        Poll::Ready(Some(Err(error)))
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

        loop {
            if this.driver.needs_input() {
                let Some(body) = this.body.as_mut() else {
                    let error = this.driver.terminate(RecordErrorKind::Internal);
                    return this.fail(error);
                };
                match Pin::new(body).poll_next(context) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => {
                        this.body = None;
                        if let Err(error) = this.driver.finish_input() {
                            return this.fail(error);
                        }
                    }
                    Poll::Ready(Some(Err(error))) => {
                        drop(error);
                        let error = this.driver.terminate(RecordErrorKind::Body);
                        return this.fail(error);
                    }
                    Poll::Ready(Some(Ok(chunk))) => {
                        if let Err(error) = this.driver.supply_input(chunk) {
                            return this.fail(error);
                        }
                    }
                }
                continue;
            }

            match this.driver.advance() {
                Ok(RecordStep::Record(record)) => return Poll::Ready(Some(Ok(record))),
                Ok(RecordStep::NeedInput) => {}
                Ok(RecordStep::Done) => {
                    this.body = None;
                    return Poll::Ready(None);
                }
                Err(error) => return this.fail(error),
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
            .field("body_open", &self.body.is_some())
            .field("driver", &self.driver)
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
            .field("stream", &self.stream)
            .field("batch_size", &self.size)
            .field("pending_records", &self.pending.len())
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
    use std::task::{Wake, Waker};

    use bytes::{Bytes, BytesMut};

    use super::*;
    use crate::endpoint::EndpointId;
    use crate::error::{ApiError, ErrorCategory};
    use crate::http::application::{ApplicationByteStream, bounded_byte_stream};
    use crate::limits::MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN;

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

    #[derive(Clone, Copy)]
    enum DecoderBehavior {
        SomeWithoutConsumption,
        SomeFromEmpty,
        Growth,
        EofSomeWithoutConsumption,
        EofGrowth,
        ConsumeNoneThenYield,
    }

    struct ContractDecoder {
        behavior: DecoderBehavior,
        calls: Arc<AtomicUsize>,
        consumed_without_record: bool,
    }

    impl ContractDecoder {
        fn new(behavior: DecoderBehavior, calls: &Arc<AtomicUsize>) -> Self {
            Self {
                behavior,
                calls: Arc::clone(calls),
                consumed_without_record: false,
            }
        }
    }

    impl RecordDecoder for ContractDecoder {
        type Record = Vec<u8>;

        fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
            unreachable!("contract decoder is constructed directly")
        }

        fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.behavior {
                DecoderBehavior::SomeWithoutConsumption if !buffer.is_empty() => {
                    Ok(Some(buffer.to_vec()))
                }
                DecoderBehavior::SomeFromEmpty => {
                    Ok(Some(b"unconsumed-record-sentinel-6f218d".to_vec()))
                }
                DecoderBehavior::Growth => {
                    buffer.extend_from_slice(b"decoder-growth-sentinel-a18c4d");
                    Ok(None)
                }
                DecoderBehavior::ConsumeNoneThenYield if !buffer.is_empty() => {
                    if self.consumed_without_record {
                        Ok(Some(buffer.split_to(1).to_vec()))
                    } else {
                        let consumed = buffer.split_to(1);
                        drop(consumed);
                        self.consumed_without_record = true;
                        Ok(None)
                    }
                }
                DecoderBehavior::SomeWithoutConsumption
                | DecoderBehavior::EofSomeWithoutConsumption
                | DecoderBehavior::EofGrowth
                | DecoderBehavior::ConsumeNoneThenYield => Ok(None),
            }
        }

        fn decode_eof(
            &mut self,
            buffer: &mut BytesMut,
        ) -> Result<Option<Self::Record>, RecordError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.behavior {
                DecoderBehavior::EofSomeWithoutConsumption => Ok(Some(buffer.to_vec())),
                DecoderBehavior::EofGrowth => {
                    buffer.extend_from_slice(b"decoder-eof-growth-sentinel-0b63cc");
                    Ok(None)
                }
                DecoderBehavior::SomeWithoutConsumption
                | DecoderBehavior::SomeFromEmpty
                | DecoderBehavior::Growth
                | DecoderBehavior::ConsumeNoneThenYield => Ok(None),
            }
        }

        fn max_buffered_bytes() -> NonZeroUsize {
            NonZeroUsize::new(64).expect("positive contract-test bound")
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

    #[derive(Default)]
    struct WakeCounter {
        wakes: AtomicUsize,
    }

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn stream(
        chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        staging: usize,
    ) -> RecordStream<LineDecoder> {
        stream_with_decoder(LineDecoder, chunks, polls, dropped, staging)
    }

    fn stream_with_decoder<D>(
        decoder: D,
        chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        staging: usize,
    ) -> RecordStream<D>
    where
        D: RecordDecoder,
    {
        let body: ApplicationByteStream = Box::pin(ProbeBody {
            chunks: chunks.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
        });
        let body = ByteStream::new(bounded_byte_stream(
            body,
            EndpointId::from_static("records.custom"),
            http::Method::GET,
            1024,
        ));
        RecordStream::new(
            body,
            decoder,
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
    async fn decoder_some_without_consumption_fails() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::SomeWithoutConsumption, &calls),
            [Ok(Bytes::from_static(b"record"))],
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut records)
            .await
            .expect("terminal error")
            .expect_err("record without progress");
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn decoder_some_from_empty_buffer_fails() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::SomeFromEmpty, &calls),
            Vec::<Result<Bytes, ApiError>>::new(),
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut records)
            .await
            .expect("terminal error")
            .expect_err("empty-buffer record");
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn decoder_growth_fails() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::Growth, &calls),
            Vec::<Result<Bytes, ApiError>>::new(),
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut records)
            .await
            .expect("terminal error")
            .expect_err("decoder growth");
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        assert!(records.driver.staging_is_empty());
    }

    #[tokio::test]
    async fn decode_eof_some_without_consumption_fails() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::EofSomeWithoutConsumption, &calls),
            [Ok(Bytes::from_static(b"unterminated"))],
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut records)
            .await
            .expect("terminal error")
            .expect_err("EOF record without progress");
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn decode_eof_growth_fails() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::EofGrowth, &calls),
            Vec::<Result<Bytes, ApiError>>::new(),
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut records)
            .await
            .expect("terminal error")
            .expect_err("EOF decoder growth");
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(records.driver.staging_is_empty());
    }

    #[tokio::test]
    async fn decoder_none_with_consumption_is_retried() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::ConsumeNoneThenYield, &calls),
            [Ok(Bytes::from_static(b"xy"))],
            &polls,
            &dropped,
            64,
        );

        assert_eq!(
            next(&mut records).await.expect("record").expect("valid"),
            b"y"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert!(records.driver.staging_is_empty());
    }

    #[tokio::test]
    async fn decoder_none_without_consumption_polls_body() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut records = stream([Ok(Bytes::from_static(b"polled\n"))], &polls, &dropped, 64);

        assert_eq!(
            next(&mut records).await.expect("record").expect("valid"),
            b"polled"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn record_stream_does_not_spin_over_ready_empty_chunks() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = std::iter::repeat_with(|| Ok(Bytes::new()))
            .take(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN + 1)
            .collect::<Vec<_>>();
        chunks.push(Ok(Bytes::from_static(b"record\n")));
        let mut records = stream(chunks, &polls, &dropped, 64);
        let wakes = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&wakes));
        let mut context = Context::from_waker(&waker);

        assert!(matches!(
            Pin::new(&mut records).poll_next(&mut context),
            Poll::Pending
        ));
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
        assert_eq!(wakes.wakes.load(Ordering::SeqCst), 1);

        let Poll::Ready(Some(Ok(record))) = Pin::new(&mut records).poll_next(&mut context) else {
            panic!("the next bounded poll yields the staged record");
        };
        assert_eq!(record, b"record");
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN + 2
        );
    }

    #[tokio::test]
    async fn valid_decoder_consumes_and_yields() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut records = stream(
            [Ok(Bytes::from_static(b"first\nsecond\n"))],
            &polls,
            &dropped,
            64,
        );

        assert_eq!(
            next(&mut records).await.expect("first").expect("valid"),
            b"first"
        );
        assert_eq!(
            next(&mut records).await.expect("second").expect("valid"),
            b"second"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn one_source_chunk_larger_than_staging_yields_all_small_records() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut records = stream(
            [Ok(Bytes::from_static(b"a\nb\nc\nd\ne\nf\n"))],
            &polls,
            &dropped,
            2,
        );

        for expected in [b"a", b"b", b"c", b"d", b"e", b"f"] {
            assert_eq!(
                next(&mut records).await.expect("record").expect("valid"),
                expected
            );
        }
        assert!(next(&mut records).await.is_none());
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn contract_failure_is_terminal() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::SomeFromEmpty, &calls),
            Vec::<Result<Bytes, ApiError>>::new(),
            &polls,
            &dropped,
            64,
        );

        assert!(next(&mut records).await.expect("one error").is_err());
        let calls_after_failure = calls.load(Ordering::SeqCst);
        assert!(next(&mut records).await.is_none());
        assert!(next(&mut records).await.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), calls_after_failure);
    }

    #[tokio::test]
    async fn contract_failure_releases_body_and_staging() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::SomeWithoutConsumption, &calls),
            [Ok(Bytes::from_static(b"staged-release-sentinel-509ba2"))],
            &polls,
            &dropped,
            64,
        );

        assert!(next(&mut records).await.expect("one error").is_err());
        assert!(records.body.is_none());
        assert!(records.driver.staging_is_empty());
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn contract_failure_error_is_redacted() {
        const SENTINEL: &[u8] = b"record-and-buffer-secret-sentinel-fab784";
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut records = stream_with_decoder(
            ContractDecoder::new(DecoderBehavior::SomeWithoutConsumption, &calls),
            [Ok(Bytes::from_static(SENTINEL))],
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut records)
            .await
            .expect("terminal error")
            .expect_err("contract error");
        let rendered = format!("{error} {error:?}");
        assert_eq!(error.kind(), RecordErrorKind::Internal);
        assert!(!rendered.contains("record-and-buffer-secret-sentinel"));
        assert!(!rendered.contains("ContractDecoder"));
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
            EndpointId::from_static("records_api.custom"),
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
