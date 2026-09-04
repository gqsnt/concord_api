use std::sync::atomic::{AtomicUsize, Ordering};

use crate::retry::{RetryDecision, RetryFailure, RetryPolicy};

use super::*;

static SIMPLE_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static SIMPLE_START_CALLS: AtomicUsize = AtomicUsize::new(0);
static SIMPLE_DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static SIMPLE_EOF_CALLS: AtomicUsize = AtomicUsize::new(0);
static SIMPLE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct SimpleRecordDecoder;

impl RecordDecoder for SimpleRecordDecoder {
    type Record = Bytes;

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-records")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        SIMPLE_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        if head
            .headers
            .get(http::header::CONTENT_TYPE)
            .is_none_or(|value| value != "application/x-concord-records")
        {
            return Err(RecordError::new(RecordErrorKind::InvalidMediaType));
        }
        Ok(())
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        SIMPLE_START_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        SIMPLE_DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
        let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        Ok(Some(record.freeze()))
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        SIMPLE_EOF_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok((!buffer.is_empty()).then(|| buffer.split_to(buffer.len()).freeze()))
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(8).expect("positive simple-record staging bound")
    }
}

fn reset_simple_decoder() {
    SIMPLE_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    SIMPLE_START_CALLS.store(0, Ordering::SeqCst);
    SIMPLE_DECODE_CALLS.store(0, Ordering::SeqCst);
    SIMPLE_EOF_CALLS.store(0, Ordering::SeqCst);
}

fn simple_headers() -> http::HeaderMap {
    record_headers(
        "application/x-concord-records",
        "buffered-record-header-secret",
    )
}

#[tokio::test]
async fn buffered_records_collect_completely_and_complete_as_success() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("first\nsecond\nlast")
                .headers(simple_headers())
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("buffered record response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let response: ApiResponse<Vec<Bytes>> =
        response_request::<BufferedRecords<SimpleRecordDecoder>>(
            core,
            Some(Duration::from_secs(30)),
        )
        .response()
        .await
        .expect("complete buffered records");

    assert_eq!(
        response.value(),
        &[
            Bytes::from_static(b"first"),
            Bytes::from_static(b"second"),
            Bytes::from_static(b"last"),
        ]
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), RESPONSE_ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert!(body_polls.load(Ordering::SeqCst) >= 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(SIMPLE_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(SIMPLE_START_CALLS.load(Ordering::SeqCst), 1);
    assert!(SIMPLE_DECODE_CALLS.load(Ordering::SeqCst) >= 3);
    assert_eq!(SIMPLE_EOF_CALLS.load(Ordering::SeqCst), 2);
    assert!(matches!(
        hooks.records().as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Response(StatusCode::OK),
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Success,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
}

#[tokio::test]
async fn buffered_records_empty_body_returns_an_empty_vector() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()).headers(simple_headers()))
        .expect("empty buffered record response");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );

    let records = response_request::<BufferedRecords<SimpleRecordDecoder>>(core, None)
        .await
        .expect("empty buffered records");
    assert!(records.is_empty());
    assert_eq!(SIMPLE_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(SIMPLE_START_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(SIMPLE_EOF_CALLS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn buffered_record_media_decode_staging_and_internal_failures_keep_categories() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    let _failure_guard = FAILURE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let media_executor = DeterministicExecutor::new();
    media_executor
        .application()
        .queue_response(
            ScriptedResponse::success("media-body-secret")
                .headers(record_headers("text/plain", "media-header-secret")),
        )
        .expect("media failure response");
    let media_core = development_core(
        &media_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let media_error = response_request::<BufferedRecords<SimpleRecordDecoder>>(media_core, None)
        .await
        .expect_err("media mismatch");
    assert_eq!(media_error.category(), ErrorCategory::ResponseContract);

    for (decoder_mode, expected) in [
        (1, ErrorCategory::ResponseDecoding),
        (2, ErrorCategory::ResponseDecoding),
    ] {
        FAILURE_MODE.store(decoder_mode, Ordering::SeqCst);
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_response(
                ScriptedResponse::success("record-error-body-secret").headers(record_headers(
                    "application/x-concord-records",
                    "decode-header-secret",
                )),
            )
            .expect("decoder failure response");
        let core = development_core(
            &executor,
            Arc::new(ProbeHooks::new()),
            RuntimeConfigBuilder::new(),
        );
        let error = response_request::<BufferedRecords<FailingRecordDecoder>>(core, None)
            .await
            .expect_err("buffered decoder failure");
        assert_eq!(error.category(), expected);
        assert_omits(
            &recursive_error_surfaces(&error),
            &["record-error-body-secret", "decode-header-secret"],
        );
    }
    FAILURE_MODE.store(0, Ordering::SeqCst);

    let staging_executor = DeterministicExecutor::new();
    staging_executor
        .application()
        .queue_response(ScriptedResponse::success("123456789").headers(simple_headers()))
        .expect("staging failure response");
    let staging_core = development_core(
        &staging_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let staging_error =
        response_request::<BufferedRecords<SimpleRecordDecoder>>(staging_core, None)
            .await
            .expect_err("one logical record exceeds staging");
    assert_eq!(staging_error.category(), ErrorCategory::ResponseDecoding);

    let internal_executor = DeterministicExecutor::new();
    internal_executor
        .application()
        .queue_response(ScriptedResponse::success("x"))
        .expect("internal failure response");
    let internal_core = development_core(
        &internal_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let internal_error =
        response_request::<BufferedRecords<NoProgressRecordDecoder>>(internal_core, None)
            .await
            .expect_err("decoder record without consumption is internal");
    assert_eq!(internal_error.category(), ErrorCategory::Internal);
}

struct NoProgressRecordDecoder;

impl RecordDecoder for NoProgressRecordDecoder {
    type Record = ();

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        Ok((!buffer.is_empty()).then_some(()))
    }

    fn decode_eof(&mut self, _buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        Ok(None)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(8).expect("positive no-progress staging bound")
    }
}

#[tokio::test]
async fn aggregate_body_failures_precede_buffered_record_conversion() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("known-excess-body-secret").headers(simple_headers()),
        )
        .expect("known excess response");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(4).expect("positive body limit")),
    );

    let error = response_request::<BufferedRecords<SimpleRecordDecoder>>(core, None)
        .await
        .expect_err("aggregate body limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(SIMPLE_VALIDATE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SIMPLE_START_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SIMPLE_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_omits(
        &recursive_error_surfaces(&error),
        &["known-excess-body-secret"],
    );
}

#[tokio::test]
async fn accepted_bytes_skips_buffered_record_validation_start_and_decode() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("raw-record-body-secret")
                .headers(record_headers("text/plain", "raw-header-secret")),
        )
        .expect("raw record response");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );

    let response = response_request::<BufferedRecords<SimpleRecordDecoder>>(core, None)
        .accepted_bytes()
        .await
        .expect("raw response bytes");
    assert_eq!(
        response.value(),
        &Bytes::from_static(b"raw-record-body-secret")
    );
    assert_eq!(response.metadata().endpoint(), RESPONSE_ENDPOINT);
    assert_eq!(SIMPLE_VALIDATE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SIMPLE_START_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SIMPLE_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SIMPLE_EOF_CALLS.load(Ordering::SeqCst), 0);
}

struct AcceptedNotFoundBufferedRecords;

impl TestResponseRepresentation for AcceptedNotFoundBufferedRecords {
    type Output = Vec<Bytes>;

    fn accepted_status() -> AcceptedStatus {
        AcceptedStatus::codes([StatusCode::NOT_FOUND]).expect("nonempty test status set")
    }
}

impl ResponseRepresentation<Vec<Bytes>> for AcceptedNotFoundBufferedRecords {
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        <BufferedRecords<SimpleRecordDecoder> as ResponseRepresentation<Vec<Bytes>>>::accept()
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        <BufferedRecords<SimpleRecordDecoder> as ResponseRepresentation<Vec<Bytes>>>::validate(head)
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Vec<Bytes>, ResponseBodyError> {
        <BufferedRecords<SimpleRecordDecoder> as ResponseRepresentation<Vec<Bytes>>>::from_body(
            body, head,
        )
    }
}

#[tokio::test]
async fn an_application_spec_can_preserve_accepted_non_success_record_statuses() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(
            ScriptedResponse::status(
                StatusCode::NOT_FOUND,
                Bytes::from_static(b"missing\nrecord"),
            )
            .headers(simple_headers()),
        )
        .expect("accepted 404 record response");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );

    let response = response_request::<AcceptedNotFoundBufferedRecords>(core, None)
        .response()
        .await
        .expect("accepted non-success buffered records");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.value(),
        &[
            Bytes::from_static(b"missing"),
            Bytes::from_static(b"record")
        ]
    );
}

#[tokio::test]
async fn rejected_retry_status_does_not_open_the_record_decoder() {
    let _guard = SIMPLE_TEST_LOCK.lock().await;
    reset_simple_decoder();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"intermediate-body-secret"),
        ))
        .expect("retryable status");
    executor
        .application()
        .queue_response(ScriptedResponse::success("after\nretry").headers(simple_headers()))
        .expect("retry record success");
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .classify_fn(|attempt| {
            assert!(matches!(
                attempt.failure(),
                RetryFailure::RejectedStatus {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    ..
                }
            ));
            RetryDecision::Retry
        })
        .build()
        .expect("bounded retry policy");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().retry_policy(policy),
    );

    let records = response_request::<BufferedRecords<SimpleRecordDecoder>>(core, None)
        .await
        .expect("retry succeeds");
    assert_eq!(
        records,
        [Bytes::from_static(b"after"), Bytes::from_static(b"retry")]
    );
    assert_eq!(SIMPLE_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(SIMPLE_START_CALLS.load(Ordering::SeqCst), 1);
}

struct SlowRecordDecoder;

impl RecordDecoder for SlowRecordDecoder {
    type Record = Bytes;

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        if buffer.is_empty() {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(30));
        Ok(Some(buffer.split_to(buffer.len()).freeze()))
    }

    fn decode_eof(&mut self, _buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        Ok(None)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64).expect("positive slow decoder staging bound")
    }
}

#[tokio::test]
async fn buffered_record_conversion_is_followed_by_the_operation_deadline_check() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success("slow-record"))
        .expect("slow decoder response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let error = response_request::<BufferedRecords<SlowRecordDecoder>>(
        core,
        Some(Duration::from_millis(5)),
    )
    .await
    .expect_err("synchronous record conversion exceeded deadline");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&hooks.records()).1, CompletionOutcome::Timeout);
}
