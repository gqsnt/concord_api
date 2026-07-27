use super::*;

struct AcceptedNotFoundStream;

impl ResponseBodySpec for AcceptedNotFoundStream {
    type Output = ByteStream;
    type Mode = Streaming;

    fn accepts_status(status: StatusCode) -> bool {
        status == StatusCode::NOT_FOUND
    }

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }
}

fn stream_headers(content_type: &'static str) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(content_type),
    );
    headers
}

#[tokio::test]
async fn accepted_non_2xx_stream_opens_with_its_actual_status() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::NOT_FOUND,
            Bytes::from_static(b"accepted-stream"),
        ))
        .expect("accepted stream response");

    let response = response_request::<AcceptedNotFoundStream>(core, Some(Duration::from_secs(30)))
        .response()
        .await
        .expect("accepted non-2xx stream");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );

    let mut stream = response.into_value();
    assert_eq!(
        next_stream_chunk(&mut stream)
            .await
            .expect("accepted stream chunk")
            .expect("valid accepted stream chunk"),
        Bytes::from_static(b"accepted-stream")
    );
    assert!(next_stream_chunk(&mut stream).await.is_none());
}

fn server_stream_headers(content_type: &str) -> Vec<(String, String)> {
    vec![("Content-Type".to_owned(), content_type.to_owned())]
}

#[tokio::test]
async fn media_unconstrained_stream_keeps_streaming_lifecycle_and_limits() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(4).expect("positive stream limit")),
    );
    let mut headers = stream_headers("not a media type");
    headers.insert(
        "x-stream-sentinel",
        http::HeaderValue::from_static("bare-stream-header"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success("bare").headers(headers))
        .expect("media-unconstrained stream response");

    let response = stream_request::<AnyMedia>(core)
        .response()
        .await
        .expect("media-unconstrained stream opens");
    assert_eq!(
        response.headers()["x-stream-sentinel"],
        "bare-stream-header"
    );
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
    let captures = executor
        .application()
        .captures()
        .expect("media-unconstrained stream capture");
    assert_eq!(captures.len(), 1);
    assert!(
        captures[0]
            .headers()
            .iter()
            .all(|header| header.name() != http::header::ACCEPT),
        "bare Stream must not insert Accept"
    );
    let mut stream = response.into_value();
    assert_eq!(
        next_stream_chunk(&mut stream)
            .await
            .expect("bare stream chunk")
            .expect("valid bare stream chunk"),
        Bytes::from_static(b"bare")
    );
    assert!(next_stream_chunk(&mut stream).await.is_none());

    let limited_executor = DeterministicExecutor::new();
    let limited_hooks = Arc::new(ProbeHooks::new());
    let limited_core = development_core(
        &limited_executor,
        Arc::clone(&limited_hooks),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(4).expect("positive stream limit")),
    );
    limited_executor
        .application()
        .queue_response(ScriptedResponse::success("excess"))
        .expect("known-length excess response");
    let error = stream_request::<AnyMedia>(limited_core)
        .await
        .expect_err("known-length bare stream excess");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(
        completion(&limited_hooks.records()).1,
        CompletionOutcome::ResponseBody
    );
}

#[tokio::test]
async fn stream_response_carries_metadata_and_completes_once_when_opened() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut headers = stream_headers("application/octet-stream");
    headers.insert(
        "x-stream-sentinel",
        http::HeaderValue::from_static("stream-header-secret-c62f"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success("stream-value-secret-8d0a").headers(headers))
        .expect("stream response script");

    let response = stream_request::<OctetStream>(core)
        .response()
        .await
        .expect("stream response");
    assert_eq!(
        response.url().as_str(),
        "http://execution.example.test/v1/stream/download"
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), STREAM_ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(
        response.headers()["x-stream-sentinel"],
        "stream-header-secret-c62f"
    );
    let records = hooks.records();
    assert_eq!(
        completion(&records).1,
        CompletionOutcome::StreamOpened,
        "completion must describe stream creation, not consumption"
    );
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);

    let debug = format!("{response:?}");
    for forbidden in [
        "stream-value-secret-8d0a",
        "stream-header-secret-c62f",
        "execution.example.test",
        "/stream/download",
    ] {
        assert!(
            !debug.contains(forbidden),
            "stream Debug leaked {forbidden}"
        );
    }
    let mut response = response.into_value();
    assert_eq!(
        next_stream_chunk(&mut response)
            .await
            .expect("stream chunk")
            .expect("valid chunk"),
        b"stream-value-secret-8d0a"[..]
    );
    assert!(next_stream_chunk(&mut response).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stream_response_known_length_and_media_fail_before_body_polling() {
    let known_gate = BodyGate::closed();
    let known_server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: server_stream_headers("application/octet-stream"),
        body: b"known-excess-secret-f45c".to_vec(),
        gate: known_gate.clone(),
    }])
    .expect("known stream server");
    let known_core = real_core(
        &known_server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(known_core),
    )
    .await
    .expect("known excess must not await the body")
    .expect_err("known stream excess");
    known_gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("known-excess-secret-f45c"));

    let media_gate = BodyGate::closed();
    let media_server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: server_stream_headers("text/plain"),
        body: b"media-mismatch-secret-18bf".to_vec(),
        gate: media_gate.clone(),
    }])
    .expect("media stream server");
    let media_core = real_core(
        &media_server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(media_core),
    )
    .await
    .expect("media mismatch must not await the body")
    .expect_err("stream media mismatch");
    media_gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{error:?} {error}").contains("media-mismatch-secret-18bf"));
}

#[tokio::test]
async fn invalid_stream_marker_is_rejected_preflight_and_required_parameters_are_enforced() {
    let invalid_executor = DeterministicExecutor::new();
    let invalid_hooks = Arc::new(ProbeHooks::new());
    let invalid_core = development_core(
        &invalid_executor,
        Arc::clone(&invalid_hooks),
        RuntimeConfigBuilder::new(),
    );
    let error = stream_request::<InvalidStreamMedia>(invalid_core)
        .await
        .expect_err("invalid media marker");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(invalid_executor.application().request_count().unwrap(), 0);
    assert!(invalid_hooks.records().iter().all(|record| !matches!(
        record,
        LifecycleRecord::Before { .. } | LifecycleRecord::Response(..)
    )));

    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_response(ScriptedResponse::success("vendor").headers(stream_headers(
            "application/vnd.concord.stream; charset=utf-8; version=1",
        )))
        .expect("valid vendor media");
    let mut accepted = stream_request::<VendorStream>(core.clone())
        .await
        .expect("required media parameter accepted with extras");
    assert_eq!(
        next_stream_chunk(&mut accepted)
            .await
            .expect("vendor chunk")
            .expect("vendor chunk success"),
        b"vendor"[..]
    );

    executor
        .application()
        .queue_response(
            ScriptedResponse::success("must-not-be-yielded")
                .headers(stream_headers("application/vnd.concord.stream; version=2")),
        )
        .expect("invalid vendor media");
    let error = stream_request::<VendorStream>(core)
        .await
        .expect_err("required media parameter mismatch");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
}

#[tokio::test]
async fn unknown_stream_is_lazy_and_first_cumulative_excess_chunk_is_withheld() {
    let lazy_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedBodyWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"ab".to_vec(), b"cd".to_vec()],
            gate: lazy_gate.clone(),
        },
        ResponseScript::Chunked {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"abc".to_vec(), b"first-excess-secret-4fa2".to_vec()],
        },
    ])
    .expect("unknown stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(1).expect("positive limit"))
            .stream_response_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let mut lazy = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(core.clone()),
    )
    .await
    .expect("unknown stream must return before body gate")
    .expect("lazy stream response");
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
    lazy_gate.release();
    assert_eq!(
        next_stream_chunk(&mut lazy).await.unwrap().unwrap(),
        b"ab"[..]
    );
    assert_eq!(
        next_stream_chunk(&mut lazy).await.unwrap().unwrap(),
        b"cd"[..]
    );
    assert!(next_stream_chunk(&mut lazy).await.is_none());

    let mut overflow = stream_request::<OctetStream>(core)
        .await
        .expect("overflow stream opens");
    assert_eq!(
        next_stream_chunk(&mut overflow).await.unwrap().unwrap(),
        b"abc"[..]
    );
    let error = next_stream_chunk(&mut overflow)
        .await
        .expect("overflow result")
        .expect_err("first excess chunk withheld");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("first-excess-secret-4fa2"));
    assert!(next_stream_chunk(&mut overflow).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
    assert!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
            .all(|record| matches!(
                record,
                LifecycleRecord::Complete {
                    outcome: CompletionOutcome::StreamOpened,
                    ..
                }
            ))
    );
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn response_stream_limit_counts_reqwest_post_decompression_bytes() {
    const GZIP_128_X: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 171, 168, 24, 88, 0, 0, 55, 138, 224, 172, 128, 0, 0, 0,
    ];
    let server = ScriptedServer::start([ResponseScript::Complete {
        status: 200,
        headers: vec![
            ("Content-Encoding".into(), "gzip".into()),
            ("Content-Type".into(), "application/octet-stream".into()),
        ],
        body: GZIP_128_X.to_vec(),
    }])
    .expect("compressed stream server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(64).expect("positive limit")),
    );
    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("compressed stream opens");
    let error = next_stream_chunk(&mut response)
        .await
        .expect("post-decompression limit result")
        .expect_err("128 decoded bytes exceed a 64-byte stream limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(next_stream_chunk(&mut response).await.is_none());
}

#[tokio::test]
async fn response_stream_native_failure_is_safe_and_emits_no_second_completion() {
    let server = ScriptedServer::start([ResponseScript::TruncatedWithHeaders {
        status: 200,
        declared_length: 6,
        prefix: b"short".to_vec(),
        headers: server_stream_headers("application/octet-stream"),
    }])
    .expect("truncated stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("truncated stream opens after valid head");
    let mut saw_native_error = None;
    while let Some(chunk) = next_stream_chunk(&mut response).await {
        match chunk {
            Ok(bytes) => assert_eq!(bytes, b"short"[..]),
            Err(error) => {
                saw_native_error = Some(error);
                break;
            }
        }
    }
    let error = saw_native_error.expect("truncation must become a stream item error");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("short"));
    assert!(next_stream_chunk(&mut response).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
}

#[tokio::test]
async fn caller_stream_polling_is_outside_operation_deadline_and_drop_is_terminal() {
    let deadline_gate = BodyGate::closed();
    let drop_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedBodyWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"after-deadline".to_vec()],
            gate: deadline_gate.clone(),
        },
        ResponseScript::GatedChunked {
            status: 200,
            first_chunk: b"first".to_vec(),
            remaining_chunk: b"must-not-be-observed".to_vec(),
            gate: drop_gate.clone(),
        },
    ])
    .expect("deadline and drop stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut after_deadline = stream_request::<OctetStream>(core.clone())
        .timeout(Duration::from_millis(100))
        .await
        .expect("stream opens inside deadline");
    tokio::time::sleep(Duration::from_millis(180)).await;
    deadline_gate.release();
    assert_eq!(
        next_stream_chunk(&mut after_deadline)
            .await
            .expect("post-deadline chunk")
            .expect("post-deadline polling is allowed"),
        b"after-deadline"[..]
    );
    assert!(next_stream_chunk(&mut after_deadline).await.is_none());

    let mut dropped = stream_request::<AnyMedia>(core)
        .await
        .expect("media-unconstrained drop stream opens");
    assert_eq!(
        next_stream_chunk(&mut dropped).await.unwrap().unwrap(),
        b"first"[..]
    );
    drop(dropped);
    drop_gate.release();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
    assert!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
            .all(|record| matches!(
                record,
                LifecycleRecord::Complete {
                    outcome: CompletionOutcome::StreamOpened,
                    ..
                }
            ))
    );
}

#[cfg(feature = "records")]
#[tokio::test]
async fn records_response_validates_starts_unpolled_and_completes_stream_opened_once() {
    OPENING_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    OPENING_START_CALLS.store(0, Ordering::SeqCst);
    OPENING_DECODE_CALLS.store(0, Ordering::SeqCst);
    OPENING_EOF_CALLS.store(0, Ordering::SeqCst);
    OPENING_DECODER_DROPS.store(0, Ordering::SeqCst);

    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("opening-record-body-sentinel-0dd64a\n")
                .headers(record_headers(
                    "application/x-concord-records",
                    "opening-header-sentinel-762db1",
                ))
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record opening response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter {
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));

    let records =
        surface_request::<Records<OpeningRecordDecoder>>(core, "opening-query-sentinel-4e15b9")
            .await
            .expect("record stream opens");

    assert_eq!(OPENING_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(OPENING_START_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(OPENING_DECODE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(OPENING_EOF_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(OPENING_DECODER_DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 0);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1
    );
    let lifecycle = hooks.records();
    assert!(matches!(
        lifecycle.as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Response(_, StatusCode::OK),
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::StreamOpened,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
    assert_eq!(completion_count(&lifecycle), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    let automatic = format!(
        "{:?}\n{:?}\n{}\n{records:?}",
        lifecycle,
        limiter.request_records(),
        debug.lock().expect("record opening debug").join("\n"),
    );
    assert_omits(
        &automatic,
        &[
            "opening-record-body-sentinel-0dd64a",
            "opening-header-sentinel-762db1",
            "opening-query-sentinel-4e15b9",
        ],
    );

    drop(records);
    assert_eq!(OPENING_DECODER_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn record_stream_body_and_decoder_failures_are_items_without_second_completion() {
    let _failure_guard = FAILURE_TEST_LOCK.lock().await;
    FAILURE_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    FAILURE_START_CALLS.store(0, Ordering::SeqCst);
    FAILURE_DECODE_CALLS.store(0, Ordering::SeqCst);
    FAILURE_EOF_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::new())
                .headers(record_headers(
                    "application/x-concord-records",
                    "failure-header-secret-58ca11",
                ))
                .body_read_failure("body-read-source-sentinel-4a60de"),
        )
        .expect("body failure response");
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("decoder-framing-body-sentinel-a14f2b").headers(
                record_headers(
                    "application/x-concord-records",
                    "failure-header-secret-58ca11",
                ),
            ),
        )
        .expect("decoder failure response");
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("decoder-eof-body-sentinel-72b59c").headers(record_headers(
                "application/x-concord-records",
                "failure-header-secret-58ca11",
            )),
        )
        .expect("EOF failure response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );

    FAILURE_MODE.store(0, Ordering::SeqCst);
    let mut body_failure = surface_request::<Records<FailingRecordDecoder>>(
        core.clone(),
        "failure-query-secret-7a184e",
    )
    .await
    .expect("body-failure stream opens");
    assert_eq!(completion_count(&hooks.records()), 1);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
    let error = next_record(&mut body_failure)
        .await
        .expect("body failure item")
        .expect_err("body read fails after opening");
    assert_eq!(error.kind(), RecordErrorKind::Body);
    assert_omits(
        &recursive_error_surfaces(&error),
        &[
            "body-read-source-sentinel-4a60de",
            "failure-header-secret-58ca11",
            "failure-query-secret-7a184e",
        ],
    );
    assert_eq!(completion_count(&hooks.records()), 1);

    FAILURE_MODE.store(1, Ordering::SeqCst);
    let mut decode_failure = surface_request::<Records<FailingRecordDecoder>>(
        core.clone(),
        "failure-query-secret-7a184e",
    )
    .await
    .expect("decode-failure stream opens");
    assert_eq!(completion_count(&hooks.records()), 2);
    let error = next_record(&mut decode_failure)
        .await
        .expect("decode failure item")
        .expect_err("decoder framing fails after opening");
    assert_eq!(error.kind(), RecordErrorKind::InvalidFraming);
    assert_omits(
        &recursive_error_surfaces(&error),
        &["decoder-framing-body-sentinel-a14f2b"],
    );
    assert_eq!(completion_count(&hooks.records()), 2);

    FAILURE_MODE.store(2, Ordering::SeqCst);
    let mut eof_failure =
        surface_request::<Records<FailingRecordDecoder>>(core, "failure-query-secret-7a184e")
            .await
            .expect("EOF-failure stream opens");
    assert_eq!(completion_count(&hooks.records()), 3);
    let error = next_record(&mut eof_failure)
        .await
        .expect("EOF failure item")
        .expect_err("decoder EOF fails after opening");
    assert_eq!(error.kind(), RecordErrorKind::IncompleteRecord);
    assert_omits(
        &recursive_error_surfaces(&error),
        &["decoder-eof-body-sentinel-72b59c"],
    );
    let lifecycle = hooks.records();
    assert_eq!(completion_count(&lifecycle), 3);
    assert!(
        lifecycle
            .iter()
            .filter_map(|record| match record {
                LifecycleRecord::Complete { outcome, .. } => Some(outcome),
                _ => None,
            })
            .all(|outcome| *outcome == CompletionOutcome::StreamOpened)
    );
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 3);
    assert_eq!(FAILURE_VALIDATE_CALLS.load(Ordering::SeqCst), 3);
    assert_eq!(FAILURE_START_CALLS.load(Ordering::SeqCst), 3);
    assert!(FAILURE_DECODE_CALLS.load(Ordering::SeqCst) >= 5);
    assert_eq!(FAILURE_EOF_CALLS.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn dropping_record_stream_synchronously_drops_decoder_staging_and_byte_stream() {
    STREAM_DROP_DECODER_DROPS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("stream-drop-record-sentinel-15f80a")
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record-stream drop response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let stream =
        surface_request::<Records<StreamDropDecoder>>(core, "stream-drop-query-sentinel-f81d0a")
            .await
            .expect("record stream opens before drop");
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 0);
    assert_eq!(STREAM_DROP_DECODER_DROPS.load(Ordering::SeqCst), 0);
    assert_eq!(completion_count(&hooks.records()), 1);

    drop(stream);
    assert_eq!(STREAM_DROP_DECODER_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(completion_count(&hooks.records()), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn dropping_record_batches_synchronously_drops_record_stream_and_byte_stream() {
    BATCH_DROP_DECODER_DROPS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success("batch-drop-record-sentinel-a7f302")
                .body_poll_counter(Arc::clone(&body_polls))
                .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("record-batches drop response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoopRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let stream =
        surface_request::<Records<BatchDropDecoder>>(core, "batch-drop-query-sentinel-46d31c")
            .await
            .expect("record stream opens before batching");
    let batches = stream.batches(NonZeroUsize::new(2).expect("positive batch"));
    assert_omits(
        &format!("{batches:?}"),
        &[
            "batch-drop-record-sentinel-a7f302",
            "batch-drop-query-sentinel-46d31c",
        ],
    );
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 0);
    assert_eq!(BATCH_DROP_DECODER_DROPS.load(Ordering::SeqCst), 0);

    drop(batches);
    assert_eq!(BATCH_DROP_DECODER_DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(completion_count(&hooks.records()), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "records")]
#[tokio::test]
async fn record_payloads_never_enter_hooks_debug_limiter_completion_or_sources() {
    let executor = DeterministicExecutor::new();
    let record_body = format!(
        "{RECORD_RAW_SENTINEL},{RECORD_CSV_HEADER_SENTINEL},{RECORD_CSV_FIELD_SENTINEL},{{\"value\":\"{RECORD_NDJSON_SENTINEL}\"}}\nparser-trigger-{RECORD_PARSER_SENTINEL}\n"
    );
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(record_body.clone()).headers(record_headers(
                "application/x-concord-records",
                RECORD_HEADER_SENTINEL,
            )),
        )
        .expect("diagnostic record response");
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(format!("invalid-media-{RECORD_RAW_SENTINEL}"))
                .headers(record_headers("text/plain", RECORD_HEADER_SENTINEL)),
        )
        .expect("diagnostic invalid-media response");
    let hooks = Arc::new(AutomaticSurfaceHooks::default());
    let limiter = Arc::new(AutomaticSurfaceLimiter::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = automatic_surface_core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        Arc::clone(&debug),
    );

    let response: ApiResponse<RecordStream<DiagnosticRecordDecoder>> =
        surface_request::<Records<DiagnosticRecordDecoder>>(core.clone(), RECORD_QUERY_SENTINEL)
            .response()
            .await
            .expect("diagnostic record stream opens");
    let response_debug = format!("{response:?}");
    let stream = response.into_value();
    let stream_debug = format!("{stream:?}");
    let mut batches = stream.batches(NonZeroUsize::new(1).expect("positive diagnostic batch"));
    let batches_debug = format!("{batches:?}");
    let selected = next_batch(&mut batches)
        .await
        .expect("selected record batch")
        .expect("selected record succeeds");
    assert_eq!(selected.len(), 1);
    let selected_debug = format!("{:?}", selected[0]);
    for expected in [
        RECORD_RAW_SENTINEL,
        RECORD_CSV_HEADER_SENTINEL,
        RECORD_CSV_FIELD_SENTINEL,
        RECORD_NDJSON_SENTINEL,
    ] {
        assert!(selected_debug.contains(expected));
    }
    let record_error = next_batch(&mut batches)
        .await
        .expect("record error batch item")
        .expect_err("parser failure remains a record item");
    assert_eq!(record_error.kind(), RecordErrorKind::InvalidFraming);
    let record_error_surfaces = recursive_error_surfaces(&record_error);
    assert_eq!(hooks.outcomes(), [CompletionOutcome::StreamOpened]);

    let api_error =
        surface_request::<Records<DiagnosticRecordDecoder>>(core, RECORD_QUERY_SENTINEL)
            .await
            .expect_err("invalid record media is an endpoint error");
    assert_eq!(api_error.category(), ErrorCategory::ResponseContract);
    let api_error_surfaces = recursive_error_surfaces(&api_error);
    assert_eq!(
        hooks.outcomes(),
        [
            CompletionOutcome::StreamOpened,
            CompletionOutcome::ResponseContract,
        ]
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 2);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 2);
    let captures = executor.application().captures().expect("record captures");
    let automatic = format!(
        "{}\n{:?}\n{:?}\n{response_debug}\n{stream_debug}\n{batches_debug}\n{record_error_surfaces}\n{api_error_surfaces}",
        debug.lock().expect("record debug output").join("\n"),
        hooks.entries(),
        limiter.entries(),
    ) + &format!("\n{captures:?}");
    assert_omits(
        &automatic,
        &[
            RECORD_RAW_SENTINEL,
            RECORD_CSV_HEADER_SENTINEL,
            RECORD_CSV_FIELD_SENTINEL,
            RECORD_NDJSON_SENTINEL,
            RECORD_DECODER_STATE_SENTINEL,
            RECORD_PARSER_SENTINEL,
            RECORD_HEADER_SENTINEL,
            RECORD_QUERY_SENTINEL,
        ],
    );
}

#[cfg(feature = "records")]
#[tokio::test]
async fn response_payloads_never_enter_hooks_debug_limiter_completion_or_wrapper_debug() {
    const BUFFERED_BODY: &str = "response-buffered-body-sentinel-c48a09";
    const CUSTOM_OUTPUT: &str = "response-custom-output-sentinel-815fce";
    const STREAM_BODY: &str = "response-byte-stream-sentinel-369d0b";
    const RECORD_BODY: &str = "response-record-value-sentinel-5a903c";
    const HEADER_VALUE: &str = "response-header-value-sentinel-1928de";
    const QUERY_VALUE: &str = "response-query-value-sentinel-61f32a";
    const PARSER_TEXT: &str = "response-parser-text-sentinel-11fa62";

    let mut buffered_headers = http::HeaderMap::new();
    buffered_headers.insert(
        "x-response-secret",
        http::HeaderValue::from_static(HEADER_VALUE),
    );
    let mut stream_headers = buffered_headers.clone();
    stream_headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/octet-stream"),
    );
    let executor = DeterministicExecutor::new();
    for response in [
        ScriptedResponse::success(BUFFERED_BODY).headers(buffered_headers.clone()),
        ScriptedResponse::success(CUSTOM_OUTPUT).headers(buffered_headers.clone()),
        ScriptedResponse::success(STREAM_BODY).headers(stream_headers),
        ScriptedResponse::success(format!("{RECORD_BODY}\n")).headers(buffered_headers),
    ] {
        executor
            .application()
            .queue_response(response)
            .expect("response diagnostic script");
    }
    let hooks = Arc::new(AutomaticSurfaceHooks::default());
    let limiter = Arc::new(AutomaticSurfaceLimiter::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = automatic_surface_core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        Arc::clone(&debug),
    );

    let error = surface_request::<FailingBufferedSpec>(core.clone(), QUERY_VALUE)
        .await
        .expect_err("custom parser error is reduced");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    let error_surfaces = recursive_error_surfaces(&error);

    let buffered: ApiResponse<SensitiveBufferedOutput> =
        surface_request::<SensitiveBufferedSpec>(core.clone(), QUERY_VALUE)
            .response()
            .await
            .expect("custom buffered output");
    assert_eq!(buffered.value().0, CUSTOM_OUTPUT);
    let buffered_debug = format!("{buffered:?}");

    let stream_response: ApiResponse<ByteStream> =
        surface_request::<StreamSpec<OctetStream>>(core.clone(), QUERY_VALUE)
            .response()
            .await
            .expect("byte stream output");
    let stream_wrapper_debug = format!("{stream_response:?}");
    let mut stream = stream_response.into_value();
    let stream_debug = format!("{stream:?}");
    assert_eq!(
        next_stream_chunk(&mut stream)
            .await
            .expect("stream response chunk")
            .expect("stream response bytes"),
        Bytes::from_static(STREAM_BODY.as_bytes())
    );
    assert!(next_stream_chunk(&mut stream).await.is_none());

    let record_response: ApiResponse<RecordStream<ResponseSurfaceRecordDecoder>> =
        surface_request::<Records<ResponseSurfaceRecordDecoder>>(core, QUERY_VALUE)
            .response()
            .await
            .expect("record response output");
    let record_wrapper_debug = format!("{record_response:?}");
    let record_stream = record_response.into_value();
    let record_stream_debug = format!("{record_stream:?}");
    let mut record_batches =
        record_stream.batches(NonZeroUsize::new(1).expect("positive response batch"));
    let record_batches_debug = format!("{record_batches:?}");
    let selected = next_batch(&mut record_batches)
        .await
        .expect("response record batch")
        .expect("response record output");
    assert_eq!(selected.len(), 1);
    assert!(format!("{:?}", selected[0]).contains(RECORD_BODY));

    assert_eq!(
        hooks.outcomes(),
        [
            CompletionOutcome::ResponseDecoding,
            CompletionOutcome::Success,
            CompletionOutcome::StreamOpened,
            CompletionOutcome::StreamOpened,
        ]
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 4);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 4);
    let captures = executor
        .application()
        .captures()
        .expect("response captures");
    let automatic = format!(
        "{}\n{:?}\n{:?}\n{:?}\n{error_surfaces}\n{buffered_debug}\n{stream_wrapper_debug}\n{stream_debug}\n{record_wrapper_debug}\n{record_stream_debug}\n{record_batches_debug}",
        debug.lock().expect("response debug output").join("\n"),
        hooks.entries(),
        limiter.entries(),
        captures,
    );
    assert_omits(
        &automatic,
        &[
            BUFFERED_BODY,
            CUSTOM_OUTPUT,
            STREAM_BODY,
            RECORD_BODY,
            HEADER_VALUE,
            QUERY_VALUE,
            PARSER_TEXT,
        ],
    );
}
