use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Wake, Waker};
use std::time::Duration;

use bytes::{Bytes, BytesMut};

use super::*;
use crate::endpoint::EndpointId;
use crate::error::{DispatchKnowledge, ErrorCategory, FailureStage};
use crate::limits::MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN;
use crate::response::stream_test_support::{
    SourceProbe, StreamEvents, TimedPoll, byte_stream, channel_source, context, limits,
    ready_source, timed_source,
};
use crate::response::{StreamLimits, StreamState};

const ENDPOINT: &str = "records.custom";

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

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
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
                    drop(buffer.split_to(1));
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

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
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

fn head_fixture<R>(run: impl FnOnce(&ResponseHead<'_>) -> R) -> R {
    let url = url::Url::parse("https://records.example.test/items").expect("test URL");
    let headers = http::HeaderMap::new();
    let metadata = crate::response::ExecutionMeta::new(
        EndpointId::from_static(ENDPOINT),
        http::Method::GET,
        0,
        1,
        0,
        false,
    );
    run(&ResponseHead {
        endpoint: ENDPOINT,
        method: &http::Method::GET,
        url: &url,
        status: http::StatusCode::OK,
        version: http::Version::HTTP_11,
        headers: &headers,
        content_length: None,
        metadata: &metadata,
    })
}

fn item_stream<D>(decoder: D, body: ByteStream, staging: usize) -> ItemStream<D::Record>
where
    D: RecordDecoder,
{
    let mut body = body;
    body.core.set_item_kind();
    let frame = NonZeroUsize::new(staging)
        .expect("positive test staging bound")
        .min(body.core.limits().frame);
    ItemStream {
        body,
        records: Box::new(RecordDriver::new(decoder, frame)),
    }
}

fn stream_with_decoder<D>(
    decoder: D,
    chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
    probe: &SourceProbe,
    staging: usize,
) -> ItemStream<D::Record>
where
    D: RecordDecoder,
{
    let body = byte_stream(ready_source(chunks, probe), ENDPOINT, limits(1024));
    item_stream(decoder, body, staging)
}

fn stream(
    chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
    probe: &SourceProbe,
    staging: usize,
) -> ItemStream<Vec<u8>> {
    stream_with_decoder(LineDecoder, chunks, probe, staging)
}

fn kind(error: &ApiError) -> Option<StreamErrorKind> {
    error.stream_summary().and_then(StreamSummary::error_kind)
}

#[tokio::test]
async fn decoder_some_without_consumption_fails() {
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::SomeWithoutConsumption, &calls),
        [Ok(Bytes::from_static(b"record"))],
        &probe,
        64,
    );
    let error = records
        .next()
        .await
        .expect("terminal error")
        .expect_err("no progress");
    assert_eq!(kind(&error), Some(StreamErrorKind::Internal));
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(probe.polls(), 1);
    assert!(probe.dropped());
}

#[tokio::test]
async fn decoder_some_from_empty_buffer_fails() {
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::SomeFromEmpty, &calls),
        Vec::<Result<Bytes, ApiError>>::new(),
        &probe,
        64,
    );
    let error = records
        .next()
        .await
        .expect("terminal error")
        .expect_err("empty record");
    assert_eq!(kind(&error), Some(StreamErrorKind::Internal));
    assert_eq!(probe.polls(), 0);
    assert!(probe.dropped());
}

#[tokio::test]
async fn decoder_growth_fails() {
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::Growth, &calls),
        Vec::<Result<Bytes, ApiError>>::new(),
        &probe,
        64,
    );
    let error = records
        .next()
        .await
        .expect("terminal error")
        .expect_err("growth");
    assert_eq!(kind(&error), Some(StreamErrorKind::Internal));
    assert_eq!(probe.polls(), 0);
    assert!(records.staging_is_empty());
}

#[tokio::test]
async fn decode_eof_some_without_consumption_fails() {
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::EofSomeWithoutConsumption, &calls),
        [Ok(Bytes::from_static(b"unterminated"))],
        &probe,
        64,
    );
    let error = records
        .next()
        .await
        .expect("terminal error")
        .expect_err("EOF no progress");
    assert_eq!(kind(&error), Some(StreamErrorKind::Internal));
    assert_eq!(probe.polls(), 2);
    assert!(probe.dropped());
}

#[tokio::test]
async fn decode_eof_growth_fails() {
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::EofGrowth, &calls),
        Vec::<Result<Bytes, ApiError>>::new(),
        &probe,
        64,
    );
    let error = records
        .next()
        .await
        .expect("terminal error")
        .expect_err("EOF growth");
    assert_eq!(kind(&error), Some(StreamErrorKind::Internal));
    assert_eq!(probe.polls(), 1);
    assert!(records.staging_is_empty());
}

#[tokio::test]
async fn decoder_none_with_consumption_is_retried() {
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::ConsumeNoneThenYield, &calls),
        [Ok(Bytes::from_static(b"xy"))],
        &probe,
        64,
    );
    assert_eq!(records.next().await.expect("record").expect("valid"), b"y");
    assert_eq!(probe.polls(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert!(records.staging_is_empty());
}

#[test]
fn record_stream_does_not_spin_over_ready_empty_chunks() {
    let probe = SourceProbe::default();
    let mut chunks = std::iter::repeat_with(|| Ok(Bytes::new()))
        .take(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN + 1)
        .collect::<Vec<_>>();
    chunks.push(Ok(Bytes::from_static(b"record\n")));
    let mut records = stream(chunks, &probe, 64);
    let wakes = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);

    assert!(Pin::new(&mut records).poll_next(&mut context).is_pending());
    assert_eq!(probe.polls(), MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
    assert_eq!(wakes.wakes.load(Ordering::SeqCst), 1);
    let Poll::Ready(Some(Ok(record))) = Pin::new(&mut records).poll_next(&mut context) else {
        panic!("the next bounded poll yields the staged record");
    };
    assert_eq!(record, b"record");
    assert_eq!(probe.polls(), MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN + 2);
}

#[tokio::test]
async fn one_source_chunk_larger_than_staging_yields_all_small_records() {
    let probe = SourceProbe::default();
    let mut records = stream([Ok(Bytes::from_static(b"a\nb\nc\nd\ne\nf\n"))], &probe, 2);
    for expected in [b"a", b"b", b"c", b"d", b"e", b"f"] {
        assert_eq!(
            records.next().await.expect("record").expect("valid"),
            expected
        );
    }
    assert!(records.next().await.is_none());
    assert_eq!(probe.polls(), 2);
    assert!(probe.dropped());
    let summary = records.summary();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.delivered_records(), 6);
    assert_eq!(summary.acquired_bytes(), 12);
}

#[tokio::test]
async fn buffered_and_streaming_delivery_have_one_byte_and_large_chunk_parity() {
    const BODY: &[u8] = b"alpha\nb\ncharlie\nlast";
    let buffered = crate::records::buffered::collect_records(Bytes::from_static(BODY), LineDecoder)
        .expect("buffered record collection");

    let one_byte_probe = SourceProbe::default();
    let mut one_byte_stream = stream(
        BODY.iter()
            .copied()
            .map(|byte| Ok(Bytes::copy_from_slice(&[byte]))),
        &one_byte_probe,
        8,
    );
    let mut one_byte = Vec::new();
    while let Some(record) = one_byte_stream.next().await {
        one_byte.push(record.expect("one-byte streaming record"));
    }

    let large_probe = SourceProbe::default();
    let mut large_stream = stream([Ok(Bytes::from_static(BODY))], &large_probe, 8);
    let mut large = Vec::new();
    while let Some(record) = large_stream.next().await {
        large.push(record.expect("large-chunk streaming record"));
    }

    assert_eq!(one_byte, buffered);
    assert_eq!(large, buffered);
    assert!(one_byte_probe.dropped());
    assert!(large_probe.dropped());
}

#[tokio::test]
async fn contract_failure_is_terminal_releases_everything_and_is_redacted() {
    const SENTINEL: &[u8] = b"record-and-buffer-secret-sentinel-fab784";
    let probe = SourceProbe::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut records = stream_with_decoder(
        ContractDecoder::new(DecoderBehavior::SomeWithoutConsumption, &calls),
        [Ok(Bytes::from_static(SENTINEL))],
        &probe,
        64,
    );
    let error = records
        .next()
        .await
        .expect("one error")
        .expect_err("contract error");
    let calls_after_failure = calls.load(Ordering::SeqCst);
    assert!(records.next().await.is_none());
    assert!(records.next().await.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), calls_after_failure);
    assert!(records.body_is_released());
    assert!(records.staging_is_empty());
    assert!(probe.dropped());
    let rendered = format!("{error} {error:?} {records:?}");
    assert!(!rendered.contains("record-and-buffer-secret-sentinel"));
    assert!(!rendered.contains("ContractDecoder"));
    assert_eq!(records.summary().state(), StreamState::Failed);
}

#[tokio::test]
async fn record_boundaries_are_chunk_independent_and_buffered_records_precede_another_poll() {
    let probe = SourceProbe::default();
    let mut records = stream(
        [
            Ok(Bytes::from_static(b"ab")),
            Ok(Bytes::from_static(b"c\nx\n")),
        ],
        &probe,
        8,
    );
    assert_eq!(records.next().await.unwrap().unwrap(), b"abc");
    assert_eq!(probe.polls(), 2);
    assert_eq!(records.next().await.unwrap().unwrap(), b"x");
    assert_eq!(probe.polls(), 2);
    assert!(records.next().await.is_none());
    assert!(probe.dropped());
}

#[tokio::test]
async fn v3_p11_decoder_staging_excess_is_a_frame_limit_and_is_withheld() {
    let probe = SourceProbe::default();
    let mut records = stream(
        [Ok(Bytes::from_static(b"oversized-record-sentinel\n"))],
        &probe,
        4,
    );
    let error = records.next().await.unwrap().expect_err("staging excess");
    assert_eq!(kind(&error), Some(StreamErrorKind::FrameLimit));
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(probe.dropped());
    assert!(records.next().await.is_none());
    assert!(!format!("{error} {error:?}").contains("oversized-record-sentinel"));
    assert_eq!(records.summary().delivered_records(), 0);
}

#[tokio::test]
async fn v3_p11_runtime_frame_limit_narrows_a_larger_decoder_ceiling() {
    let probe = SourceProbe::default();
    let narrow = StreamLimits {
        frame: NonZeroUsize::new(3).expect("positive frame"),
        ..limits(1024)
    };
    let body = byte_stream(
        ready_source([Ok(Bytes::from_static(b"ok\nlonger\n"))], &probe),
        ENDPOINT,
        narrow,
    );
    // The decoder allows 8 staged bytes; the runtime frame limit keeps 3.
    let mut records = item_stream(LineDecoder, body, 8);
    assert_eq!(records.next().await.unwrap().unwrap(), b"ok");
    let error = records.next().await.unwrap().expect_err("frame limit");
    assert_eq!(kind(&error), Some(StreamErrorKind::FrameLimit));
    assert_eq!(records.summary().delivered_records(), 1);
}

#[tokio::test]
async fn v3_p11_record_limit_withholds_the_excess_record() {
    let probe = SourceProbe::default();
    let two = StreamLimits {
        records: NonZeroUsize::new(2).expect("positive record limit"),
        ..limits(1024)
    };
    let body = byte_stream(
        ready_source([Ok(Bytes::from_static(b"a\nb\nsecret-third\n"))], &probe),
        ENDPOINT,
        two,
    );
    let mut records = item_stream(LineDecoder, body, 64);
    assert_eq!(records.next().await.unwrap().unwrap(), b"a");
    assert_eq!(records.next().await.unwrap().unwrap(), b"b");
    let error = records.next().await.unwrap().expect_err("record limit");
    assert_eq!(kind(&error), Some(StreamErrorKind::RecordLimit));
    assert_eq!(error.failure_stage(), FailureStage::StreamConsumption);
    assert!(!format!("{error} {error:?}").contains("secret-third"));
    assert!(records.next().await.is_none());
    let summary = records.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.delivered_records(), 2);
    assert!(probe.dropped());
}

#[tokio::test]
async fn body_failures_are_correlated_items_and_drop_cancels_the_byte_stream() {
    let probe = SourceProbe::default();
    let body_error = ApiError::classified(
        EndpointId::from_static(ENDPOINT),
        http::Method::GET,
        ErrorCategory::ResponseBody,
    );
    let mut records = stream([Err(body_error)], &probe, 8);
    let error = records.next().await.unwrap().expect_err("body error item");
    assert_eq!(kind(&error), Some(StreamErrorKind::Body));
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(probe.dropped());

    let drop_probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let body = ByteStream::open(
        ready_source(
            [Ok(Bytes::from_static(b"unpolled-record-sentinel\n"))],
            &drop_probe,
        ),
        context(ENDPOINT, limits(1024), events.clone(), Vec::new(), None),
    );
    let records = item_stream(LineDecoder, body, 32);
    assert!(!format!("{records:?}").contains("unpolled-record-sentinel"));
    drop(records);
    assert!(drop_probe.dropped());
    assert_eq!(drop_probe.polls(), 0);
    assert_eq!(
        events.outcomes(),
        [(
            crate::hooks::StreamKind::Items,
            crate::hooks::StreamOutcome::Dropped,
            0
        )]
    );
}

#[tokio::test]
async fn item_finish_before_and_after_the_final_record() {
    let probe = SourceProbe::default();
    let mut early = stream([Ok(Bytes::from_static(b"a\nb\n"))], &probe, 8);
    assert_eq!(early.next().await.unwrap().unwrap(), b"a");
    let summary = early.finish();
    assert_eq!(summary.state(), StreamState::Application);
    assert_eq!(summary.delivered_records(), 1);
    assert_eq!(
        summary.body_disposition(),
        crate::response::BodyDisposition::Partial
    );
    assert!(probe.dropped());

    let complete_probe = SourceProbe::default();
    let mut complete = stream([Ok(Bytes::from_static(b"a\nb"))], &complete_probe, 8);
    assert_eq!(complete.next().await.unwrap().unwrap(), b"a");
    assert_eq!(complete.next().await.unwrap().unwrap(), b"b");
    assert!(complete.next().await.is_none());
    let summary = complete.finish();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.delivered_records(), 2);
    assert_eq!(
        summary.body_disposition(),
        crate::response::BodyDisposition::Complete
    );
}

#[tokio::test(start_paused = true)]
async fn dropping_a_pending_record_future_keeps_the_decoder_buffer() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let body = byte_stream(source, ENDPOINT, limits(1024));
    let mut records = item_stream(LineDecoder, body, 8);
    sender.send(Ok(Bytes::from_static(b"ab"))).expect("send");
    // The partial record is staged, then the read is pending; the caller
    // abandons this next future.
    assert!(
        tokio::time::timeout(Duration::from_secs(1), records.next())
            .await
            .is_err()
    );
    assert!(!records.staging_is_empty());
    sender.send(Ok(Bytes::from_static(b"c\n"))).expect("send");
    assert_eq!(records.next().await.unwrap().unwrap(), b"abc");
    drop(sender);
    assert!(records.next().await.is_none());
    assert_eq!(records.summary().acquired_bytes(), 4);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_dropping_a_pending_record_future_cannot_reset_idle_or_counters() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let timed = StreamLimits {
        lifetime: Duration::from_secs(300),
        idle: Duration::from_secs(30),
        ..limits(1024)
    };
    let body = byte_stream(source, ENDPOINT, timed);
    let mut records = item_stream(LineDecoder, body, 8);
    let start = tokio::time::Instant::now();
    sender.send(Ok(Bytes::from_static(b"a\nb"))).expect("send");
    assert_eq!(records.next().await.unwrap().unwrap(), b"a");

    // Application processing after a delivered record is never idle.
    tokio::time::sleep(Duration::from_secs(40)).await;
    // The staged partial record `b` needs more input; the caller abandons
    // two pending reads after 10 seconds each.
    for _ in 0..2 {
        assert!(
            tokio::time::timeout(Duration::from_secs(10), records.next())
                .await
                .is_err()
        );
    }
    let summary = records.summary();
    assert_eq!(summary.state(), StreamState::Active);
    assert_eq!(summary.delivered_records(), 1);
    assert_eq!(summary.acquired_bytes(), 3);
    assert!(!records.staging_is_empty(), "the decoder buffer survived");

    // The third call reaches the 30-second idle budget 10 seconds in: the
    // abandoned calls kept charging the same pending baseline, and the
    // 40-second consumer pause was never charged.
    let error = tokio::time::timeout(Duration::from_secs(25), records.next())
        .await
        .expect("abandonment did not reset the idle baseline")
        .expect("terminal item")
        .expect_err("idle expiry");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(start.elapsed(), Duration::from_secs(40 + 30));
    let failed = error.stream_summary().expect("correlated stream summary");
    assert_eq!(failed.delivered_records(), 1);
    assert_eq!(failed.acquired_bytes(), 3);
    assert!(records.staging_is_empty());
    assert!(records.next().await.is_none());
    drop(sender);
}

// Review F02 acceptance for the framing layer: an expired pending-read idle
// deadline wins over record data, empty chunks or the end of the body that
// only become available after it (STREAM-02, STREAM-03).

fn timed(idle_seconds: u64) -> StreamLimits {
    StreamLimits {
        lifetime: Duration::from_secs(300),
        idle: Duration::from_secs(idle_seconds),
        ..limits(1024)
    }
}

/// Polls one record `next` future to `Pending` and drops only that temporary
/// future, keeping the handle, its decoder buffer and its idle baseline.
async fn abandon_one_pending_record<T>(records: &mut ItemStream<T>) {
    std::future::poll_fn(|context| {
        let mut next = Box::pin(records.next());
        assert!(
            next.as_mut().poll(context).is_pending(),
            "the record needs a pending read"
        );
        Poll::Ready(())
    })
    .await;
}

/// Opens a line-record stream over a channel source with a lifecycle probe.
fn observed_line_stream(
    probe: &SourceProbe,
    events: &Arc<StreamEvents>,
) -> (
    tokio::sync::mpsc::UnboundedSender<Result<Bytes, ApiError>>,
    ItemStream<Vec<u8>>,
) {
    let (sender, source) = channel_source(probe);
    let body = ByteStream::open(
        source,
        context(ENDPOINT, timed(30), events.clone(), Vec::new(), None),
    );
    (sender, item_stream(LineDecoder, body, 8))
}

fn assert_overdue_record_timeout(
    records: &ItemStream<Vec<u8>>,
    error: &ApiError,
    delivered: usize,
) {
    assert_eq!(kind(error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.failure_stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.endpoint(), ENDPOINT);
    let failed = error.stream_summary().expect("correlated stream summary");
    assert_eq!(failed.state(), StreamState::Failed);
    assert_eq!(failed.delivered_records(), delivered);
    let summary = records.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.error_kind(), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(summary.delivered_records(), delivered);
    assert!(records.staging_is_empty(), "staged bytes were released");
    assert!(records.body_is_released(), "the native body was released");
}

#[tokio::test(start_paused = true)]
async fn v3_p10_overdue_ready_data_cannot_complete_a_staged_record() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let (sender, mut records) = observed_line_stream(&probe, &events);
    sender.send(Ok(Bytes::from_static(b"a\nb"))).expect("send");
    assert_eq!(records.next().await.unwrap().unwrap(), b"a");
    // The staged partial record `b` needs a read that stays pending; the
    // caller drops that next future.
    abandon_one_pending_record(&mut records).await;
    tokio::time::advance(Duration::from_secs(31)).await;
    sender
        .send(Ok(Bytes::from_static(b"c\n")))
        .expect("record data arrives only after the idle limit");
    let error = records
        .next()
        .await
        .expect("terminal item")
        .expect_err("overdue data cannot complete the staged record");
    assert_overdue_record_timeout(&records, &error, 1);
    assert_eq!(records.summary().acquired_bytes(), 3);
    assert!(probe.dropped());
    assert!(records.next().await.is_none());
    assert_eq!(
        events.outcomes(),
        [(
            crate::hooks::StreamKind::Items,
            crate::hooks::StreamOutcome::Failed,
            1
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn v3_p10_overdue_eof_cannot_finalize_a_staged_tail_record() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let (sender, mut records) = observed_line_stream(&probe, &events);
    sender.send(Ok(Bytes::from_static(b"a\nb"))).expect("send");
    assert_eq!(records.next().await.unwrap().unwrap(), b"a");
    abandon_one_pending_record(&mut records).await;
    tokio::time::advance(Duration::from_secs(31)).await;
    // The end of the body (which would parse the unterminated final line
    // `b` exactly once) becomes available only after the idle limit.
    drop(sender);
    let error = records
        .next()
        .await
        .expect("an idle timeout, not the final record or a clean EOF")
        .expect_err("overdue EOF cannot finalize the staged tail");
    assert_overdue_record_timeout(&records, &error, 1);
    assert!(probe.dropped());
    assert!(records.next().await.is_none());
    assert_eq!(records.summary().state(), StreamState::Failed);
    assert_eq!(
        events.outcomes(),
        [(
            crate::hooks::StreamKind::Items,
            crate::hooks::StreamOutcome::Failed,
            1
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn v3_p10_overdue_empty_chunks_then_record_data_are_one_idle_timeout() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let (sender, mut records) = observed_line_stream(&probe, &events);
    abandon_one_pending_record(&mut records).await;
    tokio::time::advance(Duration::from_secs(31)).await;
    for _ in 0..2 {
        sender.send(Ok(Bytes::new())).expect("empty chunk");
    }
    sender
        .send(Ok(Bytes::from_static(b"late\n")))
        .expect("late record");
    let polls_before = probe.polls();
    let error = records
        .next()
        .await
        .expect("terminal item")
        .expect_err("empty chunks cannot complete an overdue read");
    assert_overdue_record_timeout(&records, &error, 0);
    assert_eq!(
        probe.polls(),
        polls_before,
        "an expired pending read is not polled again"
    );
    assert_eq!(records.summary().acquired_bytes(), 0);
    assert!(records.next().await.is_none());
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_record_data_below_the_idle_limit_completes_the_staged_record() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let (sender, mut records) = observed_line_stream(&probe, &events);
    sender.send(Ok(Bytes::from_static(b"a\nb"))).expect("send");
    assert_eq!(records.next().await.unwrap().unwrap(), b"a");
    abandon_one_pending_record(&mut records).await;
    tokio::time::advance(Duration::from_secs(29)).await;
    sender.send(Ok(Bytes::from_static(b"c\n"))).expect("send");
    assert_eq!(records.next().await.unwrap().unwrap(), b"bc");

    // A long consumer pause after a delivered record is never idle.
    tokio::time::advance(Duration::from_secs(200)).await;
    sender
        .send(Ok(Bytes::from_static(b"d")))
        .expect("final line");
    drop(sender);
    assert_eq!(records.next().await.unwrap().unwrap(), b"d");
    assert!(records.next().await.is_none());
    let summary = records.summary();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.delivered_records(), 3);
    assert_eq!(summary.acquired_bytes(), 6);
    assert_eq!(
        events.outcomes(),
        [(
            crate::hooks::StreamKind::Items,
            crate::hooks::StreamOutcome::Completed,
            3
        )]
    );
}

/// Executor-level evidence: an item stream handed to the caller fails its
/// overdue read once and never resends the request.
mod executor_handoff {
    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::gate::Gate;
    use concord_test_support::script::ScriptedResponse;

    use super::*;
    use crate::body::EmptyBody;
    use crate::client::ClientCore;
    use crate::http::HttpConfigBuilder;
    use crate::request::{EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta};
    use crate::runtime::RuntimeConfigBuilder;

    const GATED_ENDPOINT: &str = "records.gated_export";

    struct GatedExport;

    impl EndpointSpec for GatedExport {
        type Input = ();
        type Body = EmptyBody;
        type Output = RecordStream<LineDecoder>;
        type Response = Records<LineDecoder>;

        fn request_meta(&self) -> RequestMeta {
            RequestMeta::new(
                GATED_ENDPOINT,
                http::Method::GET,
                Some(Duration::from_secs(30)),
            )
        }

        fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
            builder.push_static("export")?;
            builder.finish()
        }

        fn into_body(self) {}
    }

    fn development_core(executor: &DeterministicExecutor) -> ClientCore {
        #[cfg(not(feature = "default-tls"))]
        let _ = rustls::crypto::ring::default_provider().install_default();
        ClientCore::build_with_development(
            "http://records.example.test/v1/",
            HttpConfigBuilder::new().http1_only(),
            RuntimeConfigBuilder::new(),
            executor,
        )
        .expect("development core")
    }

    #[tokio::test(start_paused = true)]
    async fn v3_p10_overdue_record_body_after_an_abandoned_read_is_one_idle_timeout_without_resend()
    {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .application()
            .queue_response(ScriptedResponse::success("late\nrecords\n").body_gate(gate.clone()))
            .expect("gated record response");
        executor
            .application()
            .queue_response(ScriptedResponse::success("must-not-be-requested\n"))
            .expect("unused resend response");
        let mut records = Request::new(development_core(&executor), GatedExport)
            .into_future()
            .await
            .expect("the accepted head opens the record stream");
        let operation_id = records.summary().operation_id();

        abandon_one_pending_record(&mut records).await;
        tokio::time::advance(Duration::from_secs(31)).await;
        gate.open();
        let error = records
            .next()
            .await
            .expect("terminal item")
            .expect_err("the overdue read is an idle timeout, not a record");
        assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
        assert_eq!(error.category(), ErrorCategory::Timeout);
        assert_eq!(error.operation_id(), Some(operation_id));
        assert_eq!(error.endpoint(), GATED_ENDPOINT);
        assert!(records.next().await.is_none());
        let summary = records.summary();
        assert_eq!(summary.state(), StreamState::Failed);
        assert_eq!(summary.delivered_records(), 0);
        assert_eq!(
            executor
                .application()
                .request_count()
                .expect("request count"),
            1,
            "a timed-out record stream never resends its request"
        );
        assert_eq!(
            executor
                .application()
                .remaining_scripts()
                .expect("remaining scripts"),
            1
        );
    }
}

#[tokio::test(start_paused = true)]
async fn lease_expiry_also_bounds_delivery_of_already_staged_records() {
    let probe = SourceProbe::default();
    let short = StreamLimits {
        lifetime: Duration::from_secs(10),
        ..limits(1024)
    };
    let body = byte_stream(
        ready_source([Ok(Bytes::from_static(b"a\nb\n"))], &probe),
        ENDPOINT,
        short,
    );
    let mut records = item_stream(LineDecoder, body, 8);
    assert_eq!(records.next().await.unwrap().unwrap(), b"a");
    tokio::time::sleep(Duration::from_secs(11)).await;
    let error = records.next().await.unwrap().expect_err("lease expired");
    assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
    assert!(records.staging_is_empty());
    assert_eq!(records.summary().delivered_records(), 1);
}

// Post-fix F02 acceptance for the framing layer: record data or an end of
// body observed to complete a read only after the total lease end cannot
// complete a staged record (STREAM-02).
#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_a_read_completing_after_the_lease_end_cannot_complete_a_staged_record()
{
    let cases = [
        ("record data", Some(Ok(Bytes::from_static(b"c\n"))), 5),
        ("end of body", None, 3),
    ];
    for (case, completion, acquired) in cases {
        let probe = SourceProbe::default();
        let events = Arc::new(StreamEvents::default());
        let short = StreamLimits {
            lifetime: Duration::from_millis(400),
            ..limits(1024)
        };
        let body = ByteStream::open(
            timed_source(
                [
                    TimedPoll::ready_after(Duration::ZERO, Some(Ok(Bytes::from_static(b"a\nb")))),
                    TimedPoll::pending(),
                    TimedPoll::ready_after(Duration::from_millis(700), completion),
                ],
                &probe,
            ),
            context(ENDPOINT, short, events.clone(), Vec::new(), None),
        );
        let mut records = item_stream(LineDecoder, body, 8);
        assert_eq!(records.next().await.unwrap().unwrap(), b"a");
        // The staged partial record `b` needs a read that stays pending; the
        // caller drops that next future and resumes inside the lease.
        abandon_one_pending_record(&mut records).await;
        let error = match records.next().await {
            Some(Err(error)) => error,
            Some(Ok(record)) => panic!(
                "{case}: a {}-byte record completed after the lease",
                record.len()
            ),
            None => panic!("{case}: a clean end reported after the lease"),
        };
        assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired), "{case}");
        assert_eq!(error.category(), ErrorCategory::Timeout);
        assert_eq!(error.failure_stage(), FailureStage::StreamConsumption);
        let summary = records.summary();
        assert_eq!(summary.state(), StreamState::Failed);
        assert_eq!(summary.error_kind(), Some(StreamErrorKind::LeaseExpired));
        assert_eq!(summary.delivered_records(), 1);
        assert_eq!(summary.acquired_bytes(), acquired, "{case}");
        assert!(records.staging_is_empty(), "{case}: staged bytes released");
        assert!(records.body_is_released(), "{case}: native body released");
        assert!(probe.dropped());
        assert!(records.next().await.is_none());
        assert_eq!(
            events.outcomes(),
            [(
                crate::hooks::StreamKind::Items,
                crate::hooks::StreamOutcome::Failed,
                1
            )],
            "{case}"
        );
    }
}

#[tokio::test]
async fn batching_yields_full_then_final_partial_batches() {
    let probe = SourceProbe::default();
    let mut batches = stream([Ok(Bytes::from_static(b"a\nb\nc\n"))], &probe, 8)
        .batches(NonZeroUsize::new(2).expect("positive batch"));
    assert_eq!(
        std::future::poll_fn(|context| Pin::new(&mut batches).poll_next(context))
            .await
            .unwrap()
            .unwrap(),
        [b"a".to_vec(), b"b".to_vec()]
    );
    assert_eq!(
        std::future::poll_fn(|context| Pin::new(&mut batches).poll_next(context))
            .await
            .unwrap()
            .unwrap(),
        [b"c".to_vec()]
    );
    assert!(
        std::future::poll_fn(|context| Pin::new(&mut batches).poll_next(context))
            .await
            .is_none()
    );
}

#[test]
fn decode_with_starts_once_and_uses_the_decoder_ceiling() {
    let probe = SourceProbe::default();
    let body = byte_stream(
        ready_source([Ok(Bytes::from_static(b"x\n"))], &probe),
        ENDPOINT,
        limits(1024),
    );
    let records = head_fixture(|head| {
        ItemStream::<Vec<u8>>::decode_with::<LineDecoder>(body, head).expect("started")
    });
    assert_eq!(probe.polls(), 0);
    assert!(format!("{records:?}").contains("ItemStream"));
    assert_eq!(records.summary().state(), StreamState::Active);
}

#[test]
fn item_stream_is_send_and_unpin() {
    fn assert_send_unpin<T: Send + Unpin>() {}
    assert_send_unpin::<ItemStream<Vec<u8>>>();
    assert_send_unpin::<ItemBatches<Vec<u8>>>();
}

#[derive(Clone, Copy)]
enum R7SlowBoundary {
    Record,
    EofRecord,
    EofDone,
    ConsumingNone,
}

struct R7SlowDecoder {
    boundary: R7SlowBoundary,
    callbacks: Arc<AtomicUsize>,
    cancellation: Option<crate::controls::CancellationToken>,
}

impl RecordDecoder for R7SlowDecoder {
    type Record = Vec<u8>;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        unreachable!("constructed by the native lease probe")
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Vec<u8>>, RecordError> {
        if buffer.is_empty() {
            return Ok(None);
        }
        match self.boundary {
            R7SlowBoundary::Record | R7SlowBoundary::ConsumingNone => {
                self.callbacks.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(300));
                if let Some(token) = &self.cancellation {
                    token.cancel();
                }
                if matches!(self.boundary, R7SlowBoundary::ConsumingNone) {
                    let _ = buffer.split_to(1);
                    Ok(None)
                } else {
                    Ok(Some(buffer.split_to(buffer.len()).to_vec()))
                }
            }
            R7SlowBoundary::EofRecord | R7SlowBoundary::EofDone => Ok(None),
        }
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Vec<u8>>, RecordError> {
        self.callbacks.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(300));
        let value = buffer.split_to(buffer.len()).to_vec();
        Ok(matches!(self.boundary, R7SlowBoundary::EofRecord).then_some(value))
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64).unwrap()
    }
}

async fn r7_assert_decoder_boundary(boundaries: &[R7SlowBoundary]) {
    for &boundary in boundaries {
        let probe = SourceProbe::default();
        let callbacks = Arc::new(AtomicUsize::new(0));
        let body = byte_stream(
            ready_source([Ok(Bytes::from_static(b"ab"))], &probe),
            ENDPOINT,
            StreamLimits {
                lifetime: Duration::from_millis(250),
                ..limits(1024)
            },
        );
        let mut records = item_stream(
            R7SlowDecoder {
                boundary,
                callbacks: callbacks.clone(),
                cancellation: None,
            },
            body,
            64,
        );
        let outcome = records.next().await;
        assert!(
            callbacks.load(Ordering::SeqCst) > 0,
            "decoder actually ran before expiry"
        );
        let error = outcome
            .expect("one terminal lease error")
            .expect_err("withhold decoded record");
        assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
        assert_eq!(
            callbacks.load(Ordering::SeqCst),
            1,
            "stop repeated None progress at the callback boundary"
        );
        assert_eq!(records.summary().delivered_records(), 0);
        assert_eq!(records.summary().state(), StreamState::Failed);
        assert!(records.staging_is_empty());
        assert!(records.body_is_released());
        assert!(probe.dropped());
        assert!(records.next().await.is_none());
    }
}

#[tokio::test]
async fn r7_decoder_record_is_withheld_after_lease_expiry() {
    r7_assert_decoder_boundary(&[R7SlowBoundary::Record]).await;
}

#[tokio::test]
async fn r7_decoder_eof_record_is_withheld_after_lease_expiry() {
    r7_assert_decoder_boundary(&[R7SlowBoundary::EofRecord]).await;
}

#[tokio::test]
async fn r7_decoder_eof_success_is_withheld_after_lease_expiry() {
    r7_assert_decoder_boundary(&[R7SlowBoundary::EofDone]).await;
}

#[tokio::test]
async fn r7_decoder_consuming_none_stops_at_the_expired_callback_boundary() {
    r7_assert_decoder_boundary(&[R7SlowBoundary::ConsumingNone]).await;
}

#[tokio::test]
async fn r7_decoder_cancellation_during_callback_withholds_the_record_and_releases_state() {
    let token = crate::controls::CancellationToken::new();
    let probe = SourceProbe::default();
    let callbacks = Arc::new(AtomicUsize::new(0));
    let body = ByteStream::open(
        ready_source([Ok(Bytes::from_static(b"ab"))], &probe),
        context(
            ENDPOINT,
            limits(1024),
            crate::hooks::default_operation_observer(),
            vec![token.clone()],
            None,
        ),
    );
    let mut records = item_stream(
        R7SlowDecoder {
            boundary: R7SlowBoundary::Record,
            callbacks: callbacks.clone(),
            cancellation: Some(token),
        },
        body,
        64,
    );
    let error = records
        .next()
        .await
        .unwrap()
        .expect_err("cancelled callback record is withheld");
    assert_eq!(kind(&error), Some(StreamErrorKind::Cancelled));
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(records.summary().delivered_records(), 0);
    assert!(records.staging_is_empty());
    assert!(records.body_is_released());
    assert!(records.next().await.is_none());
}

#[test]
fn per_poll_empty_chunk_budget_spans_several_reads_of_one_record() {
    let probe = SourceProbe::default();
    let empties = |count: usize| {
        std::iter::repeat_with(|| Ok(Bytes::new()))
            .take(count)
            .collect::<Vec<_>>()
    };
    let mut chunks = empties(20);
    chunks.push(Ok(Bytes::from_static(b"a")));
    chunks.extend(empties(20));
    chunks.push(Ok(Bytes::from_static(b"b")));
    chunks.extend(empties(20));
    chunks.push(Ok(Bytes::from_static(b"\n")));
    let mut records = stream(chunks, &probe, 8);
    let wakes = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);

    // One caller poll handles at most 32 empties in total, even though the
    // framing layer reads several chunks while assembling one record.
    assert!(Pin::new(&mut records).poll_next(&mut context).is_pending());
    assert_eq!(probe.polls(), 20 + 1 + 12);
    assert_eq!(wakes.wakes.load(Ordering::SeqCst), 1);
    // The remaining 28 empties fit in the next caller poll's fresh budget.
    let Poll::Ready(Some(Ok(record))) = Pin::new(&mut records).poll_next(&mut context) else {
        panic!("the second caller poll assembles the record");
    };
    assert_eq!(record, b"ab");
    assert_eq!(probe.polls(), 20 + 1 + 20 + 1 + 20 + 1);
    assert_eq!(wakes.wakes.load(Ordering::SeqCst), 1);
}
