//! Executor-level evidence for the stream handoff: acquisition is not EOF,
//! later failures keep the original correlation, the finite lease is bounded
//! by caps started at the original request, request cancellation signals
//! cancel the stream, and no retry or replay happens after handoff
//! (V3-P09, V3-P10, V3-P11).

use super::*;
use crate::controls::{CallOptions, CancellationToken};
use crate::error::{DispatchKnowledge, FailureStage};
use crate::response::{BodyDisposition, StreamErrorKind, StreamState};

fn stream_request_with(core: ClientCore, options: &CallOptions) -> Request<StreamEndpoint> {
    let mut request = stream_request(core);
    request.options.apply_call_options(options);
    request
}

#[tokio::test]
async fn v3_p09_stream_opened_is_not_eof_and_a_later_body_error_keeps_the_original_contact() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::new())
                .body_read_failure("private-stream-failure-7c1e"),
        )
        .expect("failing stream response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let response = stream_request(core)
        .response()
        .await
        .expect("the accepted head opens the stream");
    assert_eq!(response.body_disposition(), BodyDisposition::StreamOpened);
    let (operation_id, outcome, status, _) = completion(&hooks.records());
    assert_eq!(outcome, CompletionOutcome::StreamOpened);
    assert_eq!(status, Some(StatusCode::OK));

    let mut stream = response.into_value();
    let opened = stream.summary();
    assert_eq!(opened.state(), StreamState::Active);
    assert_eq!(opened.operation_id(), operation_id);
    assert_eq!(opened.status(), StatusCode::OK);
    assert_eq!(opened.attempt(), AttemptInfo::INITIAL);
    assert_eq!(opened.body_disposition(), BodyDisposition::StreamOpened);
    assert_eq!(
        opened.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );

    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the body fails after acquisition");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.operation_id(), Some(operation_id));
    assert_eq!(error.attempt(), Some(AttemptInfo::INITIAL));
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_eq!(error.endpoint(), STREAM_ENDPOINT);
    assert_eq!(error.page_index(), Some(0));
    assert_eq!(stream_error_kind(&error), Some(StreamErrorKind::Body));
    assert!(!format!("{error} {error:?}").contains("private-stream-failure-7c1e"));
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Failed);

    // One operation completion (StreamOpened, never Success) and one stream
    // terminal (Failed): the same body is never reported both ways.
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    let stream_records = hooks.stream_records();
    assert_eq!(stream_records.len(), 1);
    assert_eq!(stream_records[0].operation_id, operation_id);
    assert_eq!(stream_records[0].outcome, StreamOutcome::Failed);
}

#[tokio::test]
async fn v3_p09_finish_before_eof_differs_from_actual_eof() {
    let executor = DeterministicExecutor::new();
    for _ in 0..2 {
        executor
            .application()
            .queue_response(ScriptedResponse::success("whole-body"))
            .expect("stream response");
    }
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let early = stream_request(core.clone()).await.expect("stream opens");
    let summary = early.finish();
    assert_eq!(summary.state(), StreamState::Application);
    assert_eq!(summary.body_disposition(), BodyDisposition::DiscardedUnread);
    assert_eq!(summary.acquired_bytes(), 0);

    let mut complete = stream_request(core).await.expect("stream opens");
    assert_eq!(
        complete.next().await.expect("chunk").expect("valid chunk"),
        Bytes::from_static(b"whole-body")
    );
    assert!(complete.next().await.is_none());
    let summary = complete.finish();
    assert_eq!(summary.state(), StreamState::Eof);
    assert_eq!(summary.body_disposition(), BodyDisposition::Complete);
    assert_eq!(summary.acquired_bytes(), 10);

    let outcomes = hooks
        .stream_records()
        .into_iter()
        .map(|record| record.outcome)
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [StreamOutcome::Application, StreamOutcome::Completed]
    );
}

#[tokio::test(start_paused = true)]
async fn v3_p10_whole_operation_cap_starts_at_the_original_request_and_bounds_the_lease() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_response(ScriptedResponse::success("capped-chunk").delivery_gate(gate.clone()))
        .expect("gated stream response");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let options = CallOptions::new()
        .cap_timeout(Duration::from_secs(10))
        .expect("positive cap");
    let started = tokio::time::Instant::now();
    let pending = tokio::spawn(stream_request_with(core, &options).into_future());
    // Six seconds of the ten-second cap are spent before the head arrives.
    tokio::time::sleep(Duration::from_secs(6)).await;
    gate.open();
    let mut stream = pending
        .await
        .expect("request task")
        .expect("the stream opens inside the cap");
    assert_eq!(started.elapsed(), Duration::from_secs(6));

    // A five-second consumer pause crosses the cap (11 s after the first
    // poll) although the stream's own 5-minute lease is far from over.
    tokio::time::sleep(Duration::from_secs(5)).await;
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the whole-operation cap restricts consumption");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(
        stream_error_kind(&error),
        Some(StreamErrorKind::LeaseExpired)
    );
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert!(stream.next().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn v3_p10_selected_timeout_ends_at_acquisition_without_a_cap() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_response(ScriptedResponse::success("uncapped-chunk").delivery_gate(gate.clone()))
        .expect("gated stream response");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let pending = tokio::spawn(
        stream_request(core)
            .timeout(Duration::from_secs(7))
            .expect("valid selected timeout")
            .into_future(),
    );
    tokio::time::sleep(Duration::from_secs(6)).await;
    gate.open();
    let mut stream = pending
        .await
        .expect("request task")
        .expect("the stream opens inside the selected timeout");
    // Consumption past the selected acquisition timeout is governed only by
    // the stream lease, which is not a fresh per-record timeout.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        stream.next().await.expect("chunk").expect("valid chunk"),
        Bytes::from_static(b"uncapped-chunk")
    );
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Eof);
}

#[tokio::test(start_paused = true)]
async fn call_caps_narrow_the_stream_lease_and_byte_limit() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success("lease-capped"))
        .expect("lease response");
    executor
        .application()
        .queue_response(ScriptedResponse::success("six-by"))
        .expect("byte-capped response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let lease = CallOptions::new()
        .cap_stream_lifetime(Duration::from_secs(2))
        .expect("positive stream lifetime cap");
    let mut stream = stream_request_with(core.clone(), &lease)
        .await
        .expect("stream opens");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the narrowed lease expired");
    assert_eq!(
        stream_error_kind(&error),
        Some(StreamErrorKind::LeaseExpired)
    );

    let bytes = CallOptions::new().cap_response_bytes(NonZeroUsize::new(3).expect("positive"));
    let error = stream_request_with(core, &bytes)
        .await
        .expect_err("known length exceeds the narrowed stream byte limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
}

/// Opens one stream whose accepted head arrives at once but whose body stays
/// pending until `gate` opens, and waits on its first read from the moment
/// the stream is handed to the caller. Returns the terminal error and the
/// paused-clock time the read waited.
async fn stalled_read(
    runtime: RuntimeConfigBuilder,
    options: &CallOptions,
    gate: &Gate,
) -> (ApiError, Duration) {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success("withheld-body").body_gate(gate.clone()))
        .expect("stalled stream response");
    let core = development_core(&executor, Arc::new(ProbeHooks::new()), runtime);
    let mut stream = stream_request_with(core, options)
        .await
        .expect("the accepted head opens the stream");
    let waiting = tokio::time::Instant::now();
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the stalled read ends the stream");
    let waited = waiting.elapsed();
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert!(stream.next().await.is_none());
    assert_eq!(stream.summary().state(), StreamState::Failed);
    assert_eq!(stream.summary().acquired_bytes(), 0);
    (error, waited)
}

/// STREAM-02: a non-default runtime `stream_idle_timeout` governs an opened
/// stream (not the 30-second default), and `CallOptions::cap_stream_idle`
/// narrows it further.
#[tokio::test(start_paused = true)]
async fn runtime_stream_idle_timeout_governs_and_call_cap_narrows_the_idle_budget() {
    let gate = Gate::closed();
    let runtime = || RuntimeConfigBuilder::new().stream_idle_timeout(Duration::from_secs(5));

    let (error, waited) = stalled_read(runtime(), &CallOptions::new(), &gate).await;
    assert_eq!(
        stream_error_kind(&error),
        Some(StreamErrorKind::IdleTimeout)
    );
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(waited, Duration::from_secs(5), "the configured idle budget");

    let capped = CallOptions::new()
        .cap_stream_idle(Duration::from_secs(2))
        .expect("positive idle cap");
    let (error, waited) = stalled_read(runtime(), &capped, &gate).await;
    assert_eq!(
        stream_error_kind(&error),
        Some(StreamErrorKind::IdleTimeout)
    );
    assert_eq!(waited, Duration::from_secs(2), "the call cap narrows idle");

    // A cap larger than the configured budget never widens it.
    let wider = CallOptions::new()
        .cap_stream_idle(Duration::from_secs(20))
        .expect("positive idle cap");
    let (error, waited) = stalled_read(runtime(), &wider, &gate).await;
    assert_eq!(
        stream_error_kind(&error),
        Some(StreamErrorKind::IdleTimeout)
    );
    assert_eq!(waited, Duration::from_secs(5));
}

/// STREAM-02: a non-default runtime `stream_lifetime` governs an opened
/// stream: its total lease (here shorter than the idle budget) ends a stalled
/// read at the configured lifetime, not the 5-minute default.
#[tokio::test(start_paused = true)]
async fn runtime_stream_lifetime_governs_an_opened_stream() {
    let gate = Gate::closed();
    let runtime = RuntimeConfigBuilder::new()
        .stream_lifetime(Duration::from_secs(3))
        .stream_idle_timeout(Duration::from_secs(10));
    let (error, waited) = stalled_read(runtime, &CallOptions::new(), &gate).await;
    assert_eq!(
        stream_error_kind(&error),
        Some(StreamErrorKind::LeaseExpired)
    );
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(waited, Duration::from_secs(3), "the configured lifetime");
}

#[tokio::test]
async fn request_cancellation_signal_cancels_the_opened_stream() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success("ready-body"))
        .expect("stream response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let token = CancellationToken::new();
    let mut request = stream_request(core);
    request
        .options
        .add_cancellation(token.clone())
        .expect("one signal");
    let mut stream = request.await.expect("stream opens");
    assert_eq!(stream.summary().state(), StreamState::Active);
    token.cancel();
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("the signal cancels the whole stream");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(stream_error_kind(&error), Some(StreamErrorKind::Cancelled));
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Cancelled);
    assert_eq!(summary.body_disposition(), BodyDisposition::DiscardedUnread);
    assert!(stream.next().await.is_none());
    assert_eq!(
        hooks.stream_records().last().map(|record| record.outcome),
        Some(StreamOutcome::Cancelled)
    );
}

#[tokio::test]
async fn v3_p11_no_policy_retry_or_replay_after_stream_handoff() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()).body_read_failure("mid-stream"))
        .expect("failing stream response");
    executor
        .application()
        .queue_response(ScriptedResponse::success("must-not-be-requested"))
        .expect("unused replay response");
    let classifications = Arc::new(AtomicUsize::new(0));
    let classified = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(4)
        .without_budget()
        .classify_fn(move |_| {
            classified.fetch_add(1, Ordering::SeqCst);
            crate::retry::RetryDecision::Retry
        })
        .build()
        .expect("retry-everything policy");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().retry_policy(policy),
    );

    let mut stream = stream_request(core).await.expect("stream opens");
    let error = stream
        .next()
        .await
        .expect("terminal item")
        .expect_err("mid-stream failure");
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1,
        "a failed stream is never resubmitted"
    );
    assert_eq!(classifications.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[test]
fn client_build_reports_the_invalid_stream_setting() {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let executor = DeterministicExecutor::new();
    let error = match ClientCore::build_with_development(
        "http://execution.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().stream_idle_timeout(Duration::ZERO),
        &executor,
    ) {
        Ok(_) => panic!("a zero stream idle timeout must not build"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        crate::error::ClientBuildErrorKind::RuntimeConfiguration
    );
    assert_eq!(error.input_field(), Some("stream_idle_timeout"));
}

#[cfg(feature = "records")]
struct StartFailingDecoder;

#[cfg(feature = "records")]
impl RecordDecoder for StartFailingDecoder {
    type Record = Bytes;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Err(RecordError::new(RecordErrorKind::Decode))
    }

    fn decode(&mut self, _buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        Ok(None)
    }

    fn decode_eof(&mut self, _buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        Ok(None)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64).expect("positive staging bound")
    }
}

#[cfg(feature = "records")]
#[tokio::test]
async fn a_stream_dropped_inside_a_failed_operation_emits_no_stream_event() {
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("start-failure-body\n")
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let error = surface_request::<Records<StartFailingDecoder>>(core, "start-failure")
        .await
        .expect_err("decoder start fails before the stream reaches the caller");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::ResponseDecoding
    );
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert!(
        hooks.stream_records().is_empty(),
        "a stream never delivered to the caller reports no lifecycle"
    );
}
