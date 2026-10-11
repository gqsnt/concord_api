use std::sync::Arc;
use std::task::{Wake, Waker};
use std::time::Duration;

use bytes::Bytes;

use super::test_support::{
    SourceProbe, StreamEvents, TimedPoll, byte_stream, channel_source, context, limits,
    ready_source, timed_source,
};
use super::*;
use crate::controls::CancellationToken;
use crate::endpoint::EndpointId;
use crate::error::ErrorCategory;
use crate::hooks::AttemptInfo;
use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};
use crate::operation::OperationDeadline;

const ENDPOINT: &str = "stream.download";

#[derive(Default)]
struct WakeCounter {
    wakes: std::sync::atomic::AtomicUsize,
}

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

fn empty_chunks(count: usize) -> Vec<Result<Bytes, ApiError>> {
    std::iter::repeat_with(|| Ok(Bytes::new()))
        .take(count)
        .collect()
}

fn ready(
    chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
    probe: &SourceProbe,
    byte_limit: usize,
) -> ByteStream {
    byte_stream(ready_source(chunks, probe), ENDPOINT, limits(byte_limit))
}

fn kind(error: &ApiError) -> Option<StreamErrorKind> {
    error.stream_summary().and_then(StreamSummary::error_kind)
}

fn timed_limits(lifetime: u64, idle: u64) -> StreamLimits {
    StreamLimits {
        lifetime: Duration::from_secs(lifetime),
        idle: Duration::from_secs(idle),
        ..limits(1024)
    }
}

#[tokio::test]
async fn empty_response_chunk_is_not_yielded() {
    let probe = SourceProbe::default();
    let mut stream = ready(
        [
            Ok(Bytes::new()),
            Ok(Bytes::from_static(b"application-data")),
        ],
        &probe,
        64,
    );
    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        b"application-data"[..]
    );
    assert_eq!(probe.polls(), 2);
}

#[tokio::test]
async fn single_terminal_empty_chunk_is_tolerated() {
    let probe = SourceProbe::default();
    let mut stream = ready([Ok(Bytes::new())], &probe, 64);
    assert!(stream.next().await.is_none());
    assert_eq!(probe.polls(), 2);
    assert!(probe.dropped());
    assert_eq!(stream.summary().state(), StreamState::Eof);
}

#[tokio::test]
async fn direct_byte_stream_never_exposes_empty_items() {
    let probe = SourceProbe::default();
    let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN + 1);
    chunks.push(Ok(Bytes::from_static(b"first")));
    chunks.push(Ok(Bytes::new()));
    chunks.push(Ok(Bytes::from_static(b"second")));
    let mut stream = ready(chunks, &probe, 64);
    let mut yielded = Vec::new();
    while let Some(item) = stream.next().await {
        let chunk = item.expect("bounded stream item");
        assert!(!chunk.is_empty());
        yielded.push(chunk);
    }
    assert_eq!(
        yielded,
        [Bytes::from_static(b"first"), Bytes::from_static(b"second")]
    );
    assert_eq!(stream.summary().acquired_bytes(), 11);
    assert_eq!(stream.summary().delivered_records(), 0);
}

// On the paused clock, so the idle charged right at the yield is exactly
// zero (the acquisition stays pending across a yield: real elapsed time until
// the next poll would legitimately count).
#[tokio::test(start_paused = true)]
async fn per_poll_empty_chunk_budget_returns_pending_and_wakes_once() {
    let probe = SourceProbe::default();
    let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
    chunks.push(Ok(Bytes::from_static(b"deferred")));
    let mut stream = ready(chunks, &probe, 64);
    let wakes = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&wakes));
    let mut context = Context::from_waker(&waker);

    assert!(Pin::new(&mut stream).poll_next(&mut context).is_pending());
    assert_eq!(probe.polls(), MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
    assert_eq!(wakes.wakes.load(std::sync::atomic::Ordering::SeqCst), 1);
    // The cooperative yield wakes at once: it arms no timer and by itself
    // charges no idle.
    assert!(!stream.core.lease_for_test().has_timer_for_test());
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO
    );
    let Poll::Ready(Some(Ok(chunk))) = Pin::new(&mut stream).poll_next(&mut context) else {
        panic!("the next poll yields the deferred chunk");
    };
    assert_eq!(chunk, b"deferred"[..]);
}

#[tokio::test]
async fn consecutive_empty_chunk_exact_limit_succeeds() {
    let probe = SourceProbe::default();
    let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS);
    chunks.push(Ok(Bytes::from_static(b"exact-limit")));
    let mut stream = ready(chunks, &probe, 64);
    assert_eq!(stream.next().await.unwrap().unwrap(), b"exact-limit"[..]);
    assert!(stream.next().await.is_none());
    assert_eq!(probe.polls(), MAX_EMPTY_RESPONSE_CHUNKS + 2);
    assert!(probe.dropped());
}

#[tokio::test]
async fn consecutive_empty_count_resets_on_nonempty_data() {
    let probe = SourceProbe::default();
    let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS);
    chunks.push(Ok(Bytes::from_static(b"a")));
    chunks.extend(empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS));
    chunks.push(Ok(Bytes::from_static(b"b")));
    let mut stream = ready(chunks, &probe, 64);
    assert_eq!(stream.next().await.unwrap().unwrap(), b"a"[..]);
    assert_eq!(stream.next().await.unwrap().unwrap(), b"b"[..]);
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Eof);
}

#[tokio::test]
async fn consecutive_empty_chunk_first_excess_is_terminal_and_redacted() {
    let probe = SourceProbe::default();
    let mut stream = ready(empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1), &probe, 64);
    let error = stream
        .next()
        .await
        .unwrap()
        .expect_err("first excess empty chunk");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(kind(&error), Some(StreamErrorKind::EmptyChunkLimit));
    assert_eq!(probe.polls(), MAX_EMPTY_RESPONSE_CHUNKS + 1);
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
    let rendered = format!("{error} {error:?}");
    assert!(error.http_response().is_none());
    assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
    assert!(!rendered.contains("empty chunk"));
    assert!(!rendered.contains("http://"));
}

#[tokio::test]
async fn upstream_read_failure_keeps_its_category_and_gains_stream_correlation() {
    let probe = SourceProbe::default();
    let upstream = ApiError::classified(
        EndpointId::from_static("stream.upstream"),
        http::Method::GET,
        ErrorCategory::Connect,
    );
    let mut stream = ready(
        [Ok(Bytes::new()), Ok(Bytes::new()), Err(upstream)],
        &probe,
        64,
    );
    let error = stream
        .next()
        .await
        .unwrap()
        .expect_err("safe upstream error");
    assert_eq!(error.category(), ErrorCategory::Connect);
    assert_eq!(error.endpoint(), ENDPOINT);
    assert_eq!(kind(&error), Some(StreamErrorKind::Body));
    assert_eq!(probe.polls(), 3);
    assert!(probe.dropped());
}

#[tokio::test]
async fn eof_after_empty_chunks_completes_normally() {
    let probe = SourceProbe::default();
    let mut stream = ready(empty_chunks(5), &probe, 64);
    assert!(stream.next().await.is_none());
    assert_eq!(probe.polls(), 6);
    assert!(probe.dropped());
}

#[test]
fn debug_is_non_consuming_and_drop_cancels_the_owned_body() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let stream = ByteStream::open(
        ready_source(
            [Ok(Bytes::from_static(b"stream-debug-body-secret-a119"))],
            &probe,
        ),
        context(ENDPOINT, limits(64), events.clone(), Vec::new(), None),
    );
    let debug = format!("{stream:?}");
    assert_eq!(probe.polls(), 0);
    for forbidden in ["stream-debug-body-secret-a119", "StreamApi::download"] {
        assert!(!debug.contains(forbidden));
    }
    drop(stream);
    assert!(probe.dropped());
    assert_eq!(probe.polls(), 0);
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Dropped, 0)]
    );
}

#[tokio::test]
async fn cumulative_byte_limit_withholds_first_excess_chunk() {
    let probe = SourceProbe::default();
    let mut stream = ready(
        [
            Ok(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"first-excess-body-secret")),
        ],
        &probe,
        4,
    );
    assert_eq!(stream.next().await.unwrap().unwrap(), b"abc"[..]);
    let error = stream
        .next()
        .await
        .unwrap()
        .expect_err("first excess chunk must be withheld");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(kind(&error), Some(StreamErrorKind::ByteLimit));
    assert!(probe.dropped());
    assert_eq!(probe.polls(), 2);
    assert!(stream.next().await.is_none());
    assert!(!format!("{error:?} {error}").contains("first-excess-body-secret"));
    // The excess chunk was read (acquired) but never delivered.
    assert_eq!(stream.summary().acquired_bytes(), 27);
}

#[tokio::test]
async fn v3_p09_finish_before_eof_records_application_and_never_drains() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let mut stream = ByteStream::open(
        ready_source(
            [
                Ok(Bytes::from_static(b"a")),
                Ok(Bytes::from_static(b"never-read")),
            ],
            &probe,
        ),
        context(ENDPOINT, limits(64), events.clone(), Vec::new(), None),
    );
    let opened = stream.summary();
    assert_eq!(opened.state(), StreamState::Active);
    assert_eq!(opened.body_disposition(), BodyDisposition::StreamOpened);
    assert_eq!(stream.next().await.unwrap().unwrap(), b"a"[..]);

    let finished = stream.finish();
    assert_eq!(finished.state(), StreamState::Application);
    assert_eq!(finished.body_disposition(), BodyDisposition::Partial);
    assert_eq!(finished.acquired_bytes(), 1);
    assert_eq!(finished.error_kind(), None);
    assert_eq!(probe.polls(), 1, "finish must not drain the body");
    assert!(probe.dropped());
    // The earlier snapshot is not mutated by later lifecycle changes.
    assert_eq!(opened.state(), StreamState::Active);
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Application, 1)]
    );

    let unpolled_probe = SourceProbe::default();
    let unpolled = ready([Ok(Bytes::from_static(b"x"))], &unpolled_probe, 64);
    let summary = unpolled.finish();
    assert_eq!(summary.state(), StreamState::Application);
    assert_eq!(summary.body_disposition(), BodyDisposition::DiscardedUnread);
    assert_eq!(unpolled_probe.polls(), 0);
    assert!(unpolled_probe.dropped());
}

#[tokio::test]
async fn v3_p09_finish_after_eof_preserves_eof() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let mut stream = ByteStream::open(
        ready_source([Ok(Bytes::from_static(b"whole"))], &probe),
        context(ENDPOINT, limits(64), events.clone(), Vec::new(), None),
    );
    assert_eq!(stream.next().await.unwrap().unwrap(), b"whole"[..]);
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    let summary = stream.finish();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.body_disposition(), BodyDisposition::Complete);
    assert_eq!(summary.acquired_bytes(), 5);
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Completed, 1)]
    );
}

#[tokio::test]
async fn v3_p09_terminal_failure_yields_one_correlated_error_then_none() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let stream_context = context(ENDPOINT, limits(64), events.clone(), Vec::new(), None);
    let operation_id = stream_context.correlation.operation_id;
    let failure = ApiError::classified(
        EndpointId::from_static(ENDPOINT),
        http::Method::GET,
        ErrorCategory::ResponseBody,
    );
    let mut stream = ByteStream::open(
        ready_source([Ok(Bytes::from_static(b"ok")), Err(failure)], &probe),
        stream_context,
    );
    assert_eq!(stream.next().await.unwrap().unwrap(), b"ok"[..]);
    let error = stream
        .next()
        .await
        .unwrap()
        .expect_err("one terminal error");
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.operation_id(), Some(operation_id));
    assert_eq!(error.attempt(), Some(AttemptInfo::INITIAL));
    assert_eq!(error.last_response_status(), Some(http::StatusCode::OK));
    assert_eq!(error.endpoint(), ENDPOINT);
    let summary = error.stream_summary().expect("stream summary on error");
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.operation_id(), operation_id);
    assert_eq!(summary.status(), http::StatusCode::OK);
    assert_eq!(summary.acquired_bytes(), 2);
    assert_eq!(summary.body_disposition(), BodyDisposition::Failed);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Failed);
    assert_eq!(stream.summary().error_kind(), Some(StreamErrorKind::Body));
    assert_eq!(stream.finish().state(), StreamState::Failed);
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Failed, 1)]
    );
}

#[tokio::test(start_paused = true)]
async fn v3_p10_idle_charges_only_pending_reads_not_consumer_pauses() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    let start = tokio::time::Instant::now();

    sender.send(Ok(Bytes::from_static(b"one"))).expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"one"[..]);
    // Application processing after a successful read is never idle.
    tokio::time::sleep(Duration::from_secs(40)).await;
    sender.send(Ok(Bytes::from_static(b"two"))).expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"two"[..]);

    // 29 seconds of pending read, abandoned by the caller.
    assert!(
        tokio::time::timeout(Duration::from_secs(29), stream.next())
            .await
            .is_err()
    );
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(29)
    );
    // Nonempty data resets the idle budget.
    sender.send(Ok(Bytes::from_static(b"three"))).expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"three"[..]);
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO
    );

    let error = tokio::time::timeout(Duration::from_secs(31), stream.next())
        .await
        .expect("the stream's own idle timer fires first")
        .expect("terminal item")
        .expect_err("idle expiry");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(start.elapsed(), Duration::from_secs(40 + 29 + 30));
    assert!(probe.dropped());
    assert!(!stream.core.lease_for_test().has_timer_for_test());
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().acquired_bytes(), 11);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_dropping_next_cannot_reset_idle_or_counters() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    let start = tokio::time::Instant::now();
    sender.send(Ok(Bytes::from_static(b"ab"))).expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"ab"[..]);

    for _ in 0..2 {
        assert!(
            tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .is_err()
        );
    }
    assert_eq!(stream.summary().state(), StreamState::Active);
    assert_eq!(stream.summary().acquired_bytes(), 2);
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(20)
    );
    // A resumed read keeps the original pending baseline: the idle limit is
    // reached 10 seconds into this third call, not 30.
    let error = tokio::time::timeout(Duration::from_secs(25), stream.next())
        .await
        .expect("abandonment did not reset the idle baseline")
        .expect("terminal item")
        .expect_err("idle expiry");
    assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(start.elapsed(), Duration::from_secs(30));
    assert_eq!(
        error.stream_summary().map(StreamSummary::acquired_bytes),
        Some(2)
    );
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_abandoned_next_futures_cannot_extend_the_total_lease() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    // The idle budget exceeds the total lease, so only the total lease fixed
    // at acquisition can end this stream.
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(25, 300));
    let start = tokio::time::Instant::now();
    sender.send(Ok(Bytes::from_static(b"xy"))).expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"xy"[..]);

    // Two pending reads abandoned by the caller after 10 seconds each.
    for _ in 0..2 {
        assert!(
            tokio::time::timeout(Duration::from_secs(10), stream.next())
                .await
                .is_err()
        );
    }
    assert_eq!(stream.summary().state(), StreamState::Active);
    assert_eq!(stream.summary().acquired_bytes(), 2);

    // A fresh call does not restart the lease: it expires 5 seconds in, at
    // the 25-second end fixed when the stream was acquired.
    let error = tokio::time::timeout(Duration::from_secs(20), stream.next())
        .await
        .expect("abandonment did not extend the total lease")
        .expect("terminal item")
        .expect_err("lease expiry");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
    assert_eq!(start.elapsed(), Duration::from_secs(25));
    assert_eq!(
        error.stream_summary().map(StreamSummary::acquired_bytes),
        Some(2)
    );
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_dropping_next_cannot_reset_the_consecutive_empty_chunk_counter() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    let first = MAX_EMPTY_RESPONSE_CHUNKS - 24;
    for _ in 0..first {
        sender.send(Ok(Bytes::new())).expect("send empty");
    }
    // The caller abandons the call once the empties are consumed and the
    // read is pending.
    assert!(
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .is_err()
    );
    assert!(
        probe.polls() > first,
        "every queued empty chunk was consumed"
    );
    assert_eq!(stream.summary().state(), StreamState::Active);

    // The remaining budget is still exactly tolerated across a second
    // abandoned call ...
    for _ in first..MAX_EMPTY_RESPONSE_CHUNKS {
        sender.send(Ok(Bytes::new())).expect("send empty");
    }
    assert!(
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .is_err()
    );
    assert_eq!(stream.summary().state(), StreamState::Active);

    // ... and the first consecutive excess is terminal although it arrives
    // in a third call: the counter lives in the handle, not in the future.
    sender.send(Ok(Bytes::new())).expect("send empty");
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("first excess empty chunk across abandoned calls");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(kind(&error), Some(StreamErrorKind::EmptyChunkLimit));
    assert_eq!(stream.summary().acquired_bytes(), 0);
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_empty_chunks_do_not_reset_pending_read_idle() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    let start = tokio::time::Instant::now();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), stream.next())
            .await
            .is_err()
    );
    sender.send(Ok(Bytes::new())).expect("send empty");
    let error = tokio::time::timeout(Duration::from_secs(20), stream.next())
        .await
        .expect("an empty chunk must not reset idle")
        .expect("terminal item")
        .expect_err("idle expiry");
    assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(start.elapsed(), Duration::from_secs(30));
    assert_eq!(stream.summary().acquired_bytes(), 0);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_total_lease_charges_consumer_pauses_and_releases_on_next_poll() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(60, 30));
    sender.send(Ok(Bytes::from_static(b"first"))).expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"first"[..]);
    // The consumer pauses beyond the total lease without polling; nothing
    // is released until the next poll or drop (no detached timer task).
    tokio::time::sleep(Duration::from_secs(61)).await;
    assert!(!probe.dropped());
    sender
        .send(Ok(Bytes::from_static(b"ready-but-expired")))
        .expect("send");
    let polls_before = probe.polls();
    let error = stream
        .next()
        .await
        .unwrap()
        .expect_err("lease expiry withholds ready data");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
    assert_eq!(probe.polls(), polls_before, "expired lease is not polled");
    assert!(probe.dropped());
    assert_eq!(stream.summary().acquired_bytes(), 5);
    assert!(stream.next().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn v3_p10_total_lease_expires_while_a_read_is_pending() {
    let probe = SourceProbe::default();
    let (_sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(10, 30));
    let start = tokio::time::Instant::now();
    let error = stream.next().await.unwrap().expect_err("lease expiry");
    assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
    assert_eq!(start.elapsed(), Duration::from_secs(10));
    assert!(probe.dropped());
}

#[tokio::test(start_paused = true)]
async fn v3_p10_whole_operation_cap_started_at_the_original_request_bounds_the_lease() {
    let probe = SourceProbe::default();
    let (_sender, source) = channel_source(&probe);
    // The request started 6 seconds before acquisition under a 10-second
    // whole-operation cap: only 4 seconds of lease remain, although the
    // stream's own lifetime is 300 seconds.
    let cap = OperationDeadline::now() + Duration::from_secs(4);
    let mut stream = ByteStream::open(
        source,
        context(
            ENDPOINT,
            timed_limits(300, 30),
            crate::hooks::default_operation_observer(),
            Vec::new(),
            Some(cap),
        ),
    );
    let start = tokio::time::Instant::now();
    let error = stream.next().await.unwrap().expect_err("cap expiry");
    assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
    assert_eq!(start.elapsed(), Duration::from_secs(4));
}

#[tokio::test(start_paused = true)]
async fn dropping_a_pending_next_future_resumes_the_same_acquisition() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .is_err()
    );
    assert_eq!(stream.summary().state(), StreamState::Active);
    assert!(!probe.dropped());
    sender
        .send(Ok(Bytes::from_static(b"resumed")))
        .expect("send");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"resumed"[..]);
    drop(sender);
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Eof);
}

#[tokio::test(start_paused = true)]
async fn external_cancellation_cancels_the_whole_stream_while_pending() {
    let probe = SourceProbe::default();
    let (_sender, source) = channel_source(&probe);
    let token = CancellationToken::new();
    let events = Arc::new(StreamEvents::default());
    let mut stream = ByteStream::open(
        source,
        context(
            ENDPOINT,
            timed_limits(300, 30),
            events.clone(),
            vec![token.clone()],
            None,
        ),
    );
    let canceller = token.clone();
    let cancel = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        canceller.cancel();
    });
    let error = stream.next().await.unwrap().expect_err("cancelled");
    cancel.await.expect("canceller");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(kind(&error), Some(StreamErrorKind::Cancelled));
    assert_eq!(stream.summary().state(), StreamState::Cancelled);
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Cancelled, 0)]
    );
}

#[tokio::test]
async fn external_cancellation_is_observed_before_ready_data() {
    let probe = SourceProbe::default();
    let token = CancellationToken::new();
    let mut stream = ByteStream::open(
        ready_source([Ok(Bytes::from_static(b"ready"))], &probe),
        context(
            ENDPOINT,
            limits(64),
            crate::hooks::default_operation_observer(),
            vec![token.clone()],
            None,
        ),
    );
    token.cancel();
    let error = stream.next().await.unwrap().expect_err("cancelled");
    assert_eq!(kind(&error), Some(StreamErrorKind::Cancelled));
    assert_eq!(probe.polls(), 0);
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Cancelled);
    assert_eq!(summary.body_disposition(), BodyDisposition::DiscardedUnread);
}

#[test]
fn stream_types_have_the_required_auto_traits() {
    fn assert_send_unpin<T: Send + Unpin>() {}
    fn assert_send_sync_clone<T: Send + Sync + Clone>() {}
    fn assert_copy_eq<T: Copy + Eq + std::fmt::Debug>() {}
    assert_send_unpin::<ByteStream>();
    assert_send_sync_clone::<StreamSummary>();
    assert_copy_eq::<StreamState>();
    assert_copy_eq::<StreamErrorKind>();
    fn next_future_is_send(stream: &mut ByteStream) -> impl Send + '_ {
        stream.next()
    }
    let _ = next_future_is_send;
}

// Independent review F02. Regression additions: NOT compiled by the reviewer.
// Only after idle has expired does the producer make data/EOF available.
#[tokio::test(start_paused = true)]
async fn review_f02_ready_data_cannot_erase_expired_pending_idle() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    std::future::poll_fn(|cx| {
        let mut next = Box::pin(stream.next());
        assert!(std::future::Future::poll(next.as_mut(), cx).is_pending());
        std::task::Poll::Ready(()) // drops the pending temporary, not the handle
    })
    .await;
    tokio::time::advance(Duration::from_secs(31)).await;
    sender
        .send(Ok(Bytes::from_static(b"too late")))
        .expect("producer is still connected");
    let error = stream
        .next()
        .await
        .expect("terminal error item")
        .expect_err("idle was exhausted before this data arrived");
    assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(stream.summary().state(), StreamState::Failed);
    assert!(stream.next().await.is_none());
    assert!(probe.dropped());
}

#[tokio::test(start_paused = true)]
async fn review_f02_ready_eof_cannot_erase_expired_pending_idle() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    std::future::poll_fn(|cx| {
        let mut next = Box::pin(stream.next());
        assert!(std::future::Future::poll(next.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    tokio::time::advance(Duration::from_secs(31)).await;
    drop(sender); // EOF becomes available only after the deadline
    let error = stream
        .next()
        .await
        .expect("timeout, not clean EOF")
        .expect_err("expired pending read");
    assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(stream.summary().state(), StreamState::Failed);
    assert!(stream.next().await.is_none());
}

// Review F02 acceptance: an expired pending-read idle deadline wins over any
// completion that only becomes observable after it (STREAM-02, STREAM-03).

/// Polls one `next` future to `Pending` and drops only that temporary future,
/// leaving the handle and its outstanding pending-read baseline in place.
async fn abandon_one_pending_next(stream: &mut ByteStream) {
    std::future::poll_fn(|context| {
        let mut next = Box::pin(stream.next());
        assert!(
            next.as_mut().poll(context).is_pending(),
            "the underlying read is pending"
        );
        Poll::Ready(())
    })
    .await;
}

fn assert_one_idle_timeout(error: &ApiError) {
    assert_eq!(kind(error), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.endpoint(), ENDPOINT);
    assert_eq!(error.last_response_status(), Some(http::StatusCode::OK));
}

#[tokio::test(start_paused = true)]
async fn v3_p10_overdue_empty_chunks_then_ready_data_are_one_idle_timeout() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let events = Arc::new(StreamEvents::default());
    let mut stream = ByteStream::open(
        source,
        context(
            ENDPOINT,
            timed_limits(300, 30),
            events.clone(),
            Vec::new(),
            None,
        ),
    );
    abandon_one_pending_next(&mut stream).await;
    tokio::time::advance(Duration::from_secs(31)).await;
    // Empty chunks and then data become available only after the deadline.
    for _ in 0..3 {
        sender.send(Ok(Bytes::new())).expect("empty chunk");
    }
    sender
        .send(Ok(Bytes::from_static(b"after-empties-too-late")))
        .expect("late data");
    let polls_before = probe.polls();
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("empty chunks cannot complete an overdue read");
    assert_one_idle_timeout(&error);
    assert_eq!(
        probe.polls(),
        polls_before,
        "an expired pending read is not polled again"
    );
    assert!(probe.dropped());
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.error_kind(), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(summary.body_disposition(), BodyDisposition::Failed);
    assert_eq!(summary.acquired_bytes(), 0);
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Failed, 0)]
    );
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_empty_chunk_before_the_deadline_does_not_reset_idle_for_overdue_data() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    abandon_one_pending_next(&mut stream).await;
    tokio::time::advance(Duration::from_secs(20)).await;
    // A live read consumes an empty chunk 20 seconds in and stays pending;
    // the caller abandons it 5 seconds later.
    sender.send(Ok(Bytes::new())).expect("empty chunk");
    assert!(
        tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .is_err()
    );
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(25),
        "the empty chunk did not reset the idle budget"
    );
    tokio::time::advance(Duration::from_secs(6)).await;
    sender
        .send(Ok(Bytes::from_static(b"overdue-after-empty")))
        .expect("late data");
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("20 s + 11 s of pending read exceed the idle budget");
    assert_one_idle_timeout(&error);
    assert_eq!(stream.summary().state(), StreamState::Failed);
    assert_eq!(stream.summary().acquired_bytes(), 0);
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn v3_p10_ready_data_at_exactly_the_idle_limit_is_an_idle_timeout() {
    // The owned timer of a live read fires at exactly the idle limit; a
    // resumed read observes the same boundary.
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    abandon_one_pending_next(&mut stream).await;
    tokio::time::advance(Duration::from_secs(30)).await;
    sender
        .send(Ok(Bytes::from_static(b"at-the-limit")))
        .expect("data at the limit");
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("idle reached its limit");
    assert_one_idle_timeout(&error);
    assert!(stream.next().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn v3_p10_ready_data_below_the_idle_limit_resumes_and_later_consumer_pauses_are_free() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    abandon_one_pending_next(&mut stream).await;
    tokio::time::advance(Duration::from_secs(29)).await;
    sender
        .send(Ok(Bytes::from_static(b"in-time")))
        .expect("data below the limit");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"in-time"[..]);
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO,
        "nonempty data resets the idle budget"
    );
    assert!(!stream.core.lease_for_test().has_timer_for_test());

    // Application processing after a successful read is never idle, even far
    // beyond the idle limit (the total lease still bounds it).
    tokio::time::advance(Duration::from_secs(200)).await;
    sender
        .send(Ok(Bytes::from_static(b"after-pause")))
        .expect("data after a consumer pause");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"after-pause"[..]);
    drop(sender);
    assert!(stream.next().await.is_none());
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.acquired_bytes(), 18);
    assert_eq!(probe.polls(), 4);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_empty_chunks_below_the_idle_limit_keep_charging_until_data_arrives_in_time() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    abandon_one_pending_next(&mut stream).await;
    tokio::time::advance(Duration::from_secs(20)).await;
    sender.send(Ok(Bytes::new())).expect("empty chunk");
    assert!(
        tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .is_err()
    );
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(25)
    );
    tokio::time::advance(Duration::from_secs(4)).await;
    sender
        .send(Ok(Bytes::from_static(b"just-in-time")))
        .expect("data below the limit");
    assert_eq!(stream.next().await.unwrap().unwrap(), b"just-in-time"[..]);
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO
    );
    assert_eq!(stream.summary().state(), StreamState::Active);
    drop(sender);
}

/// Pending on its first poll; on its second poll it takes `delay` of real
/// time before producing data, so the read completes inside that poll.
struct SlowCompletion {
    polled: bool,
    delay: Duration,
    probe: SourceProbe,
}

impl Stream for SlowCompletion {
    type Item = Result<Bytes, ApiError>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.probe
            .polls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if !self.polled {
            self.polled = true;
            return Poll::Pending;
        }
        std::thread::sleep(self.delay);
        Poll::Ready(Some(Ok(Bytes::from_static(b"completed-too-late"))))
    }
}

impl Drop for SlowCompletion {
    fn drop(&mut self) {
        self.probe
            .dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test]
async fn v3_p10_a_read_completing_past_the_idle_limit_within_one_poll_is_an_idle_timeout() {
    // Real clock: the resumed poll starts immediately, far inside the idle
    // budget, but the outstanding read only completes after it. Its
    // completion is judged at the instant it is observed, like a
    // still-pending read.
    let probe = SourceProbe::default();
    let source: ApplicationByteStream = Box::pin(SlowCompletion {
        polled: false,
        delay: Duration::from_millis(1000),
        probe: probe.clone(),
    });
    let mut stream = byte_stream(
        source,
        ENDPOINT,
        StreamLimits {
            idle: Duration::from_millis(500),
            ..limits(1024)
        },
    );
    abandon_one_pending_next(&mut stream).await;
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the read completed after the idle limit");
    assert_one_idle_timeout(&error);
    assert_eq!(probe.polls(), 2, "the resumed poll reached the source");
    assert!(probe.dropped());
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    // The overdue chunk was read from the transport and withheld.
    assert_eq!(summary.acquired_bytes(), 18);
    assert_eq!(
        error.stream_summary().map(StreamSummary::acquired_bytes),
        Some(18)
    );
    assert!(stream.next().await.is_none());
}

/// Executor-level evidence: a stream handed to the caller by the managed
/// executor fails its overdue read once and never resends the request.
mod executor_handoff {
    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::gate::Gate;
    use concord_test_support::script::ScriptedResponse;

    use super::*;
    use crate::body::EmptyBody;
    use crate::client::ClientCore;
    use crate::codec::Binary;
    use crate::http::HttpConfigBuilder;
    use crate::request::{EndpointSpec, HeadBuilder, PreparedHead, Request, RequestMeta};
    use crate::runtime::RuntimeConfigBuilder;

    const GATED_ENDPOINT: &str = "stream.gated_download";

    struct GatedDownload;

    impl EndpointSpec for GatedDownload {
        type Input = ();
        type Body = EmptyBody;
        type Output = ByteStream;
        type Response = Binary;

        fn request_meta(&self) -> RequestMeta {
            RequestMeta::new(
                GATED_ENDPOINT,
                http::Method::GET,
                Some(Duration::from_secs(30)),
            )
        }

        fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
            builder.push_static("download")?;
            builder.finish()
        }

        fn into_body(self) {}
    }

    fn development_core(executor: &DeterministicExecutor) -> ClientCore {
        #[cfg(not(feature = "default-tls"))]
        let _ = rustls::crypto::ring::default_provider().install_default();
        ClientCore::build_with_development(
            "http://stream.example.test/v1/",
            HttpConfigBuilder::new().http1_only(),
            RuntimeConfigBuilder::new(),
            executor,
        )
        .expect("development core")
    }

    #[tokio::test(start_paused = true)]
    async fn v3_p10_overdue_body_after_an_abandoned_read_is_one_idle_timeout_without_resend() {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        executor
            .application()
            .queue_response(ScriptedResponse::success("gated-late-body").body_gate(gate.clone()))
            .expect("gated stream response");
        executor
            .application()
            .queue_response(ScriptedResponse::success("must-not-be-requested"))
            .expect("unused resend response");
        let mut stream = Request::new(development_core(&executor), GatedDownload)
            .into_future()
            .await
            .expect("the accepted head opens the stream");
        let operation_id = stream.summary().operation_id();

        abandon_one_pending_next(&mut stream).await;
        // The default 30-second idle budget elapses before the body is ready.
        tokio::time::advance(Duration::from_secs(31)).await;
        gate.open();
        let error = stream
            .next()
            .await
            .expect("terminal item")
            .expect_err("the overdue read is an idle timeout, not data");
        assert_eq!(kind(&error), Some(StreamErrorKind::IdleTimeout));
        assert_eq!(error.category(), ErrorCategory::Timeout);
        assert_eq!(error.stage(), FailureStage::StreamConsumption);
        assert_eq!(error.operation_id(), Some(operation_id));
        assert_eq!(error.endpoint(), GATED_ENDPOINT);
        assert!(stream.next().await.is_none());
        let summary = stream.summary();
        assert_eq!(summary.state(), StreamState::Failed);
        assert_eq!(summary.acquired_bytes(), 0);
        assert_eq!(
            executor
                .application()
                .request_count()
                .expect("request count"),
            1,
            "a timed-out stream never resends its request"
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

// Post-fix review of the F02 repair (evidence/r2/postfix_review/f02): the
// reviewer's three probes, kept as regressions. A read observed to complete
// after the total lease end is withheld as `LeaseExpired`, which also takes
// precedence over an exhausted idle budget at that instant, as in `precheck`
// and `on_pending`; time away at a cooperative empty-chunk yield counts as
// pending idle (STREAM-02, STREAM-03).

/// A read completing inside one poll after both the total lease and the idle
/// budget elapsed reports the total lease, as `precheck` and `on_pending` do.
#[tokio::test]
async fn review_postfix_f02_completion_after_total_and_idle_precedence() {
    let probe = SourceProbe::default();
    let source: ApplicationByteStream = Box::pin(SlowCompletion {
        polled: false,
        delay: Duration::from_millis(1000),
        probe: probe.clone(),
    });
    let mut stream = byte_stream(
        source,
        ENDPOINT,
        StreamLimits {
            lifetime: Duration::from_millis(400),
            idle: Duration::from_millis(500),
            ..limits(1024)
        },
    );
    abandon_one_pending_next(&mut stream).await;
    let item = stream.next().await.expect("terminal item");
    let error = item.expect_err("both limits elapsed before the completion");
    assert_eq!(
        kind(&error),
        Some(StreamErrorKind::LeaseExpired),
        "total lease precedence as in precheck/on_pending"
    );
    assert!(stream.next().await.is_none());
}

/// A read completing inside one poll after the total lease end (only) is not
/// delivered after the lease end.
#[tokio::test]
async fn review_postfix_f02_completion_after_total_lease_only() {
    let probe = SourceProbe::default();
    let source: ApplicationByteStream = Box::pin(SlowCompletion {
        polled: false,
        delay: Duration::from_millis(700),
        probe: probe.clone(),
    });
    let mut stream = byte_stream(
        source,
        ENDPOINT,
        StreamLimits {
            lifetime: Duration::from_millis(400),
            idle: Duration::from_secs(30),
            ..limits(1024)
        },
    );
    abandon_one_pending_next(&mut stream).await;
    let item = stream.next().await.expect("an item");
    let error = item.expect_err("a chunk observed after the total lease end was delivered");
    assert_eq!(kind(&error), Some(StreamErrorKind::LeaseExpired));
    assert!(stream.next().await.is_none());
}

/// Abandonment at the 32-empty-chunk cooperative yield: the data acquisition
/// is still pending there, so time away is charged as idle exactly like
/// abandonment at a pending read, and late data is an idle timeout.
#[tokio::test(start_paused = true)]
async fn review_postfix_f02_abandon_at_empty_chunk_yield() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    abandon_one_pending_next(&mut stream).await; // pending_since = t0
    tokio::time::advance(Duration::from_secs(20)).await;
    for _ in 0..MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
        sender.send(Ok(Bytes::new())).expect("empty");
    }
    // One poll consumes the empties and yields cooperatively; drop it there.
    std::future::poll_fn(|context| {
        let mut next = Box::pin(stream.next());
        assert!(
            next.as_mut().poll(context).is_pending(),
            "cooperative yield"
        );
        Poll::Ready(())
    })
    .await;
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(20),
        "idle charged at the yield"
    );
    tokio::time::advance(Duration::from_secs(40)).await; // consumer away
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(60),
        "time away at a yield is pending idle"
    );
    sender.send(Ok(Bytes::from_static(b"late"))).expect("data");
    let item = stream.next().await.expect("item");
    let error = item.expect_err("20 s pending + 40 s away at a yield exceed the idle budget");
    assert_one_idle_timeout(&error);
    assert!(stream.next().await.is_none());
}

// Post-fix F02 acceptance on the paused clock: every observed completion of
// an underlying read is judged at that instant by the total lease first and
// then by the idle budget of an outstanding pending read (the order and the
// boundaries of `precheck` and of the owned timer). A withheld chunk that was
// read counts as acquired; nothing is read or counted when a limit already
// expired at the start of a poll.

/// The 18-byte chunk of every late completion below.
const LATE: &[u8] = b"completed-too-late";

fn late_chunk() -> Option<Result<Bytes, ApiError>> {
    Some(Ok(Bytes::from_static(LATE)))
}

fn millis_limits(lifetime: u64, idle: u64) -> StreamLimits {
    StreamLimits {
        lifetime: Duration::from_millis(lifetime),
        idle: Duration::from_millis(idle),
        ..limits(1024)
    }
}

/// Opens a byte stream over a scripted timed source with a lifecycle probe.
fn timed_stream(
    polls: impl IntoIterator<Item = TimedPoll>,
    probe: &SourceProbe,
    limits: StreamLimits,
    events: &Arc<StreamEvents>,
) -> ByteStream {
    ByteStream::open(
        timed_source(polls, probe),
        context(ENDPOINT, limits, events.clone(), Vec::new(), None),
    )
}

fn assert_one_lease_expiry(stream: &ByteStream, error: &ApiError, acquired: u64) {
    assert_eq!(kind(error), Some(StreamErrorKind::LeaseExpired));
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.endpoint(), ENDPOINT);
    assert_eq!(error.last_response_status(), Some(http::StatusCode::OK));
    assert_eq!(
        error.stream_summary().map(StreamSummary::acquired_bytes),
        Some(acquired)
    );
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.error_kind(), Some(StreamErrorKind::LeaseExpired));
    assert_eq!(summary.body_disposition(), BodyDisposition::Failed);
    assert_eq!(summary.acquired_bytes(), acquired);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_completion_after_both_limits_is_one_lease_expiry() {
    // The total lease wins whichever of the two limits ran out first.
    for (lifetime, idle) in [(400, 500), (500, 400)] {
        let probe = SourceProbe::default();
        let events = Arc::new(StreamEvents::default());
        let mut stream = timed_stream(
            [
                TimedPoll::pending(),
                TimedPoll::ready_after(Duration::from_millis(1000), late_chunk()),
            ],
            &probe,
            millis_limits(lifetime, idle),
            &events,
        );
        let start = tokio::time::Instant::now();
        abandon_one_pending_next(&mut stream).await;
        let error = stream
            .next()
            .await
            .expect("terminal item")
            .expect_err("both limits elapsed before the completion");
        assert_eq!(start.elapsed(), Duration::from_millis(1000));
        // The withheld chunk was read from the transport.
        assert_one_lease_expiry(&stream, &error, 18);
        assert_eq!(probe.polls(), 2, "the resumed poll reached the source");
        assert!(probe.dropped());
        assert!(stream.next().await.is_none());
        assert_eq!(
            events.outcomes(),
            [(StreamKind::Bytes, StreamOutcome::Failed, 0)]
        );
    }
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_precheck_gives_the_same_verdict_and_reads_nothing() {
    // Reference for the completion check above: when both limits ran out
    // before the next poll starts, that poll reports the total lease without
    // reading the data that merely became available.
    for (lifetime, idle) in [(400, 500), (500, 400)] {
        let probe = SourceProbe::default();
        let events = Arc::new(StreamEvents::default());
        let mut stream = timed_stream(
            [
                TimedPoll::pending(),
                TimedPoll::ready_after(Duration::ZERO, late_chunk()),
            ],
            &probe,
            millis_limits(lifetime, idle),
            &events,
        );
        abandon_one_pending_next(&mut stream).await;
        tokio::time::advance(Duration::from_millis(1000)).await;
        let error = stream
            .next()
            .await
            .expect("terminal item")
            .expect_err("both limits elapsed before the poll");
        assert_one_lease_expiry(&stream, &error, 0);
        assert_eq!(probe.polls(), 1, "an expired stream is not polled again");
        assert!(probe.dropped());
        assert!(stream.next().await.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_completion_after_the_lease_end_is_withheld_not_delivered() {
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    // The idle budget is far from exhausted; only the total lease ended.
    let mut stream = timed_stream(
        [
            TimedPoll::pending(),
            TimedPoll::ready_after(Duration::from_millis(700), late_chunk()),
        ],
        &probe,
        millis_limits(400, 30_000),
        &events,
    );
    abandon_one_pending_next(&mut stream).await;
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("a chunk read after the lease end is not delivered");
    assert_one_lease_expiry(&stream, &error, 18);
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Failed, 0)]
    );
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_completion_at_the_lease_end_is_expired_and_just_before_is_delivered() {
    // Exactly at the lease end the read is too late: the `now >= end`
    // boundary of `precheck` and of the owned timer.
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let mut stream = timed_stream(
        [
            TimedPoll::pending(),
            TimedPoll::ready_after(Duration::from_millis(400), late_chunk()),
        ],
        &probe,
        millis_limits(400, 30_000),
        &events,
    );
    abandon_one_pending_next(&mut stream).await;
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("a read completing at the lease end is too late");
    assert_one_lease_expiry(&stream, &error, 18);

    // Positive control: one millisecond earlier the same read is delivered,
    // and the stream still ends at the lease end fixed at acquisition.
    let probe = SourceProbe::default();
    let events = Arc::new(StreamEvents::default());
    let mut stream = timed_stream(
        [
            TimedPoll::pending(),
            TimedPoll::ready_after(Duration::from_millis(399), late_chunk()),
        ],
        &probe,
        millis_limits(400, 30_000),
        &events,
    );
    let start = tokio::time::Instant::now();
    abandon_one_pending_next(&mut stream).await;
    assert_eq!(stream.next().await.unwrap().unwrap(), LATE);
    assert_eq!(stream.summary().state(), StreamState::Active);
    assert_eq!(stream.summary().acquired_bytes(), 18);
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO
    );
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the lease ends while the next read is pending");
    assert_eq!(start.elapsed(), Duration::from_millis(400));
    assert_one_lease_expiry(&stream, &error, 18);
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Failed, 1)]
    );
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_any_completion_observed_after_the_lease_end_is_lease_expired() {
    // The read was never pending: it starts inside the lease and completes
    // after its end within one poll. The total lease bounds the stream's
    // lifetime at every observed completion, whatever completes the read.
    let read_failure = || {
        Some(Err(ApiError::classified(
            EndpointId::from_static("stream.upstream"),
            http::Method::GET,
            ErrorCategory::Connect,
        )))
    };
    let cases = [
        ("data", late_chunk(), 18),
        ("end of body", None, 0),
        ("read failure", read_failure(), 0),
        ("empty chunk", Some(Ok(Bytes::new())), 0),
    ];
    for (case, completion, acquired) in cases {
        let probe = SourceProbe::default();
        let events = Arc::new(StreamEvents::default());
        let mut stream = timed_stream(
            [TimedPoll::ready_after(
                Duration::from_millis(700),
                completion,
            )],
            &probe,
            millis_limits(400, 30_000),
            &events,
        );
        let item = stream.next().await;
        let error = match item {
            Some(Err(error)) => error,
            Some(Ok(chunk)) => panic!("{case}: {} bytes delivered after the lease", chunk.len()),
            None => panic!("{case}: a clean end reported after the lease"),
        };
        assert_one_lease_expiry(&stream, &error, acquired);
        assert_eq!(probe.polls(), 1, "{case}: polled once");
        assert!(probe.dropped(), "{case}: body released");
        assert!(stream.next().await.is_none());
        assert_eq!(
            events.outcomes(),
            [(StreamKind::Bytes, StreamOutcome::Failed, 0)],
            "{case}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_time_away_at_an_empty_chunk_yield_counts_as_pending_idle() {
    // No read was pending before the empty chunks: the cooperative yield
    // itself leaves the data acquisition pending.
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let events = Arc::new(StreamEvents::default());
    let mut stream = ByteStream::open(
        source,
        context(
            ENDPOINT,
            timed_limits(300, 30),
            events.clone(),
            Vec::new(),
            None,
        ),
    );
    for _ in 0..MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
        sender.send(Ok(Bytes::new())).expect("empty chunk");
    }
    // The poll consumes the empties and yields; the caller drops it there.
    abandon_one_pending_next(&mut stream).await;
    assert_eq!(probe.polls(), MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
    assert!(!stream.core.lease_for_test().has_timer_for_test());
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO,
        "the yield itself charges nothing"
    );
    tokio::time::advance(Duration::from_secs(30)).await;
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(30),
        "time away at the yield is pending idle"
    );
    sender
        .send(Ok(Bytes::from_static(b"late-after-a-yield")))
        .expect("late data");
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the idle budget ran out at the yield");
    assert_one_idle_timeout(&error);
    assert_eq!(
        probe.polls(),
        MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN,
        "an expired acquisition is not read again"
    );
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.error_kind(), Some(StreamErrorKind::IdleTimeout));
    // Rejected before the next read: the late data was neither read nor
    // counted.
    assert_eq!(summary.acquired_bytes(), 0);
    assert!(probe.dropped());
    assert!(stream.next().await.is_none());
    assert_eq!(
        events.outcomes(),
        [(StreamKind::Bytes, StreamOutcome::Failed, 0)]
    );
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn v3_p10_postfix_f02_resuming_after_an_empty_chunk_yield_within_the_budget_delivers() {
    let probe = SourceProbe::default();
    let (sender, source) = channel_source(&probe);
    let mut stream = byte_stream(source, ENDPOINT, timed_limits(300, 30));
    abandon_one_pending_next(&mut stream).await;
    tokio::time::advance(Duration::from_secs(20)).await;
    for _ in 0..MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
        sender.send(Ok(Bytes::new())).expect("empty chunk");
    }
    abandon_one_pending_next(&mut stream).await;
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(20),
        "empty chunks did not reset the cumulative idle"
    );
    tokio::time::advance(Duration::from_secs(9)).await;
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::from_secs(29)
    );
    let in_time = Bytes::from_static(b"in-time-after-a-yield");
    sender
        .send(Ok(in_time.clone()))
        .expect("data below the limit");
    assert_eq!(stream.next().await.unwrap().unwrap(), in_time);
    assert_eq!(
        stream.core.lease_for_test().idle_charged_for_test(),
        Duration::ZERO,
        "nonempty data resets the idle budget"
    );
    assert!(!stream.core.lease_for_test().has_timer_for_test());
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Active);
    assert_eq!(
        summary.acquired_bytes(),
        u64::try_from(in_time.len()).unwrap()
    );
    drop(sender);
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Eof);
}
