use super::*;

#[tokio::test]
async fn buffered_request_is_prepared_once_when_attempt_observation_panics() {
    let executor = DeterministicExecutor::new();
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/x-concord-probe"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"ok")).headers(headers))
        .expect("observer panic response");
    let mut probe = ProbeHooks::new();
    probe.panic_on_attempt = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let _output = codec_request::<ProbeEncoder, ProbeDecoder>(
        core,
        probe_input(b"observer-body-secret-78f4", &encoder_calls),
    )
    .await
    .expect("observer panic cannot veto");
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert!(matches!(
        hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Buffered,
            known_body_length: Some(25),
            ..
        })
    ));
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_aggregate_framing_limit_and_factory_failure_are_pre_native() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(64).expect("positive limit")),
    );
    let body = RequestBody::multipart(
        MultipartBody::new()
            .part(
                "field",
                MultipartPart::text("x").expect("bounded multipart text"),
            )
            .expect("multipart part"),
    );
    let error = body_request(core, body)
        .await
        .expect_err("multipart framing exceeds the request limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    assert!(matches!(
        hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Multipart,
            known_body_length: Some(1),
            ..
        })
    ));

    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        Arc::clone(&failing_hooks),
        RuntimeConfigBuilder::new(),
    );
    let create_calls = Arc::new(AtomicUsize::new(0));
    let error = body_request(
        failing_core,
        RequestBody::multipart_factory(ProbeMultipartFactory {
            create_calls: Arc::clone(&create_calls),
            fail: true,
        }),
    )
    .await
    .expect_err("multipart factory failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("multipart-factory-error-secret-88a1"));
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_stream_failure_is_sanitized_and_request_body_primary() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let stream = RequestStream::from_stream(BodyPollProbe::new(
        [ProbeFrame::Failure("multipart-producer-error-secret-9b31")],
        &polls,
    ));
    let body = RequestBody::multipart(
        MultipartBody::new()
            .part("stream", MultipartPart::stream(stream))
            .expect("multipart stream part"),
    );
    let error = body_request(core, body)
        .await
        .expect_err("multipart producer failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("multipart-producer-error-secret-9b31"));
}

#[tokio::test]
async fn managed_request_stream_enforces_exact_length_and_aggregate_limit() {
    let success_executor = DeterministicExecutor::new();
    let success_hooks = Arc::new(ProbeHooks::new());
    let success_core = development_core(
        &success_executor,
        Arc::clone(&success_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(5).expect("positive limit")),
    );
    success_executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .observed_url("http://execution.example.test/v1/body/upload")
                .body("abcde"),
            ScriptedResponse::success("accepted"),
        )
        .expect("stream success script");
    let success_polls = Arc::new(AtomicUsize::new(0));
    let success_body = RequestStream::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"ab"), ProbeFrame::Data(b"cde")],
        &success_polls,
    ))
    .exact_length(5);
    assert_eq!(
        body_request(success_core, RequestBody::stream(success_body))
            .await
            .expect("exact stream success"),
        b"accepted"[..]
    );
    assert_eq!(success_polls.load(Ordering::SeqCst), 3);
    let captures = success_executor.application().captures().unwrap();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].body_category(), BodyCategory::Streaming);
    assert_eq!(captures[0].known_body_length(), Some(5));

    let preflight_executor = DeterministicExecutor::new();
    let preflight_hooks = Arc::new(ProbeHooks::new());
    let preflight_core = development_core(
        &preflight_executor,
        Arc::clone(&preflight_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let preflight_polls = Arc::new(AtomicUsize::new(0));
    let preflight_body = RequestStream::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"never-polled")],
        &preflight_polls,
    ))
    .exact_length(5);
    let error = body_request(preflight_core, RequestBody::stream(preflight_body))
        .await
        .expect_err("known impossible stream size");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(preflight_polls.load(Ordering::SeqCst), 0);
    assert_eq!(preflight_executor.application().request_count().unwrap(), 0);
    assert!(preflight_hooks.records().iter().all(|record| !matches!(
        record,
        LifecycleRecord::Before { .. } | LifecycleRecord::Response(_)
    )));

    async fn managed_failure(
        body: RequestStream,
        polls: &Arc<AtomicUsize>,
        expected_polls: usize,
        limit: usize,
    ) -> ApiError {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let core = development_core(
            &executor,
            hooks,
            RuntimeConfigBuilder::new()
                .request_body_limit(NonZeroUsize::new(limit).expect("positive limit")),
        );
        let error = body_request(core, RequestBody::stream(body))
            .await
            .expect_err("managed stream failure");
        assert_eq!(error.category(), ErrorCategory::RequestBody);
        assert_eq!(polls.load(Ordering::SeqCst), expected_polls);
        assert_eq!(executor.application().request_count().unwrap(), 0);
        error
    }

    let underflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        RequestStream::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc")],
            &underflow_polls,
        ))
        .exact_length(4),
        &underflow_polls,
        2,
        64,
    )
    .await;

    let exact_overflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        RequestStream::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc"), ProbeFrame::Data(b"de")],
            &exact_overflow_polls,
        ))
        .exact_length(4),
        &exact_overflow_polls,
        2,
        64,
    )
    .await;

    let limit_overflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        RequestStream::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc"), ProbeFrame::Data(b"de")],
            &limit_overflow_polls,
        )),
        &limit_overflow_polls,
        2,
        4,
    )
    .await;
}

#[tokio::test]
async fn producer_failure_is_request_body_primary_and_body_free() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let body = RequestStream::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"accepted-prefix-secret-029b"),
            ProbeFrame::Failure("producer-authored-secret-80d1"),
        ],
        &polls,
    ));
    let error = body_request(core, RequestBody::stream(body))
        .await
        .expect_err("producer failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    let recursive = format!("{error:?} {error}");
    assert!(!recursive.contains("accepted-prefix-secret-029b"));
    assert!(!recursive.contains("producer-authored-secret-80d1"));
}

#[tokio::test]
async fn body_factory_is_created_once_after_approval_and_failure_is_pre_native() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .observed_url("http://execution.example.test/v1/body/upload")
                .body("factory"),
            ScriptedResponse::success("factory-ok"),
        )
        .expect("factory script");
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let request = body_request(
        core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&content_type_calls),
            create_calls: Arc::clone(&create_calls),
            polls: Arc::clone(&polls),
            fail: false,
        }),
    );
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(request.await.expect("factory request"), b"factory-ok"[..]);
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);

    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        failing_hooks,
        RuntimeConfigBuilder::new(),
    );
    let failing_content_type_calls = Arc::new(AtomicUsize::new(0));
    let failing_create_calls = Arc::new(AtomicUsize::new(0));
    let failing_polls = Arc::new(AtomicUsize::new(0));
    let error = body_request(
        failing_core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&failing_content_type_calls),
            create_calls: Arc::clone(&failing_create_calls),
            polls: Arc::clone(&failing_polls),
            fail: true,
        }),
    )
    .await
    .expect_err("factory creation failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(failing_content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_polls.load(Ordering::SeqCst), 0);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("safe factory configuration failure"));
}

#[tokio::test]
async fn bytes_reader_and_file_stream_constructors_use_the_same_native_path() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    for (expected, response) in [
        (b"bytes-source".as_slice(), "bytes-ok"),
        (b"reader-source".as_slice(), "reader-ok"),
        (b"file-source".as_slice(), "file-ok"),
    ] {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .observed_url("http://execution.example.test/v1/body/upload")
                    .body(Bytes::copy_from_slice(expected)),
                ScriptedResponse::success(response),
            )
            .expect("constructor script");
    }

    assert_eq!(
        body_request(
            core.clone(),
            RequestBody::stream(RequestStream::from_bytes(Bytes::from_static(
                b"bytes-source"
            ))),
        )
        .await
        .expect("bytes stream"),
        b"bytes-ok"[..]
    );
    assert_eq!(
        body_request(
            core.clone(),
            RequestBody::stream(
                RequestStream::from_reader(std::io::Cursor::new(b"reader-source".to_vec()))
                    .exact_length(13),
            ),
        )
        .await
        .expect("reader stream"),
        b"reader-ok"[..]
    );

    let file_path = std::env::temp_dir().join(format!(
        "concord-request-stream-{}-{}.tmp",
        std::process::id(),
        BODY_ENDPOINT.len()
    ));
    tokio::fs::write(&file_path, b"file-source")
        .await
        .expect("write stream fixture");
    let file_body = RequestStream::from_file(&file_path)
        .await
        .expect("open stream fixture");
    let file_result = body_request(core, RequestBody::stream(file_body)).await;
    tokio::fs::remove_file(&file_path)
        .await
        .expect("remove stream fixture");
    assert_eq!(file_result.expect("file stream"), b"file-ok"[..]);
    assert_eq!(executor.application().request_count().unwrap(), 3);
}
