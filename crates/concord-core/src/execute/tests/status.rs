use super::*;

#[tokio::test]
async fn buffered_codec_direct_and_metadata_results_share_one_execution_path() {
    PROBE_DECODER_CALLS.store(0, Ordering::SeqCst);
    PROBE_VALIDATOR_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let runtime = RuntimeConfigBuilder::new()
        .request_body_limit(NonZeroUsize::new(5).expect("positive limit"));
    let core = development_core(&executor, Arc::clone(&hooks), runtime);
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/codec/round-trip")
                .body("exact"),
            probe_response("first"),
        )
        .expect("direct codec response");
    let value = codec_request::<ProbeEncoder, ProbeDecoder>(
        core.clone(),
        probe_input(b"exact", &encoder_calls),
    )
    .await
    .expect("direct codec output");
    assert_eq!(value, "decoded:first");

    executor
        .application()
        .queue_response(probe_response("second"))
        .expect("metadata codec response");
    let response =
        codec_request::<ProbeEncoder, ProbeDecoder>(core, probe_input(b"again", &encoder_calls))
            .response()
            .await
            .expect("metadata codec output");
    assert_eq!(response.value(), "decoded:second");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), CODEC_ENDPOINT);
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 2);
    assert_eq!(PROBE_VALIDATOR_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(PROBE_DECODER_CALLS.load(Ordering::SeqCst), 2);
    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    for capture in captures {
        assert_eq!(capture.body_category(), BodyCategory::Bytes);
        assert_eq!(capture.known_body_length(), Some(5));
    }
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn encoding_failure_and_request_limit_prevent_native_execution_safely() {
    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        Arc::clone(&failing_hooks),
        RuntimeConfigBuilder::new(),
    );
    let failing_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        failing_core,
        EncoderInput {
            bytes: Bytes::from_static(b"request-encoding-body-secret-98d2"),
            calls: Arc::clone(&failing_calls),
            fail: true,
        },
    )
    .await
    .expect_err("encoder failure");
    assert_eq!(error.category(), ErrorCategory::RequestEncoding);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    assert_eq!(failing_hooks.completion_returned.load(Ordering::SeqCst), 1);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("request-encoding-body-secret-98d2"));
        assert!(!formatted.contains("request-codec-secret-ec2d"));
    }

    let limit_executor = DeterministicExecutor::new();
    let limit_hooks = Arc::new(ProbeHooks::new());
    let limit_core = development_core(
        &limit_executor,
        Arc::clone(&limit_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    limit_executor
        .application()
        .queue_response(ScriptedResponse::success("ok"))
        .expect("below-limit response");
    assert_eq!(
        codec_request::<Text, Text>(limit_core.clone(), "abc".to_owned())
            .await
            .expect("below request limit"),
        "ok"
    );
    let limit_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        limit_core,
        probe_input(b"first-excess-request-secret", &limit_calls),
    )
    .await
    .expect_err("request body limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(limit_calls.load(Ordering::SeqCst), 1);
    assert_eq!(limit_executor.application().request_count().unwrap(), 1);
    assert_eq!(limit_hooks.completion_returned.load(Ordering::SeqCst), 2);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("first-excess-request-secret"));
    }
}

#[tokio::test]
async fn content_type_and_decoder_failures_are_safe_and_lifecycle_ordered() {
    REJECTING_DECODER_CALLS.store(0, Ordering::SeqCst);
    let contract_executor = DeterministicExecutor::new();
    contract_executor
        .application()
        .queue_response(ScriptedResponse::success("body-must-not-reach-decoder"))
        .expect("contract response");
    let contract_core = development_core(
        &contract_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, RejectingDecoder>(
        contract_core,
        probe_input(b"input", &calls),
    )
    .await
    .expect_err("content-type mismatch");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(REJECTING_DECODER_CALLS.load(Ordering::SeqCst), 0);

    FAILING_DECODER_CALLS.store(0, Ordering::SeqCst);
    let decode_executor = DeterministicExecutor::new();
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(
            "successful-response-body-secret-0b6c",
        ))
        .expect("decode response");
    let decode_core = development_core(
        &decode_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error =
        codec_request::<ProbeEncoder, FailingDecoder>(decode_core, probe_input(b"input", &calls))
            .await
            .expect_err("decoder failure");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(FAILING_DECODER_CALLS.load(Ordering::SeqCst), 1);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("successful-response-body-secret-0b6c"));
        assert!(!formatted.contains("response-decoder-secret-c413"));
        assert!(!formatted.contains("content-type-secret-4fea"));
    }
}

#[tokio::test]
async fn preparation_failure_skips_before_send_and_transport_but_still_completes() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = Request::new(core, InvalidEndpoint)
        .await
        .expect_err("invalid route");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
        [LifecycleRecord::Complete {
            outcome: CompletionOutcome::Configuration,
            status: None,
            ..
        }]
    ));
}

#[tokio::test]
async fn transport_failures_are_safely_classified_and_complete_once() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::Connect)
        .expect("failure");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core).await.expect_err("connect failure");
    assert_eq!(error.category(), ErrorCategory::Connect);
    let records = hooks.records();
    assert_eq!(records.len(), 2);
    assert_eq!(completion(&records).1, CompletionOutcome::Connect);
}

#[tokio::test]
async fn real_reqwest_transmits_and_decodes_text_and_custom_buffered_codecs() {
    let server = ScriptedServer::start([
        ResponseScript::ok("text-response-♥".as_bytes().to_vec()),
        ResponseScript::ok(b"custom-response".to_vec())
            .with_header("Content-Type", "application/x-concord-probe"),
    ])
    .expect("codec server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let text = codec_request::<Text, Text>(core.clone(), "text-request-♥".to_owned())
        .response()
        .await
        .expect("text response");
    assert_eq!(text.value(), "text-response-♥");

    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let custom = codec_request::<ProbeEncoder, RealCustomDecoder>(
        core,
        probe_input(b"custom-request", &encoder_calls),
    )
    .await
    .expect("custom response");
    assert_eq!(custom, "custom-response");
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 1);
    server
        .wait_for_requests(2, Duration::from_secs(1))
        .expect("request recording");
    let requests = server.requests();
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].body(), "text-request-♥".as_bytes());
    assert_eq!(
        requests[0].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(requests[0].header("accept"), Some("text/plain"));
    assert_eq!(requests[1].body(), b"custom-request");
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/x-concord-probe")
    );
    assert_eq!(
        requests[1].header("accept"),
        Some("application/x-concord-probe")
    );
}

#[tokio::test]
async fn buffered_decoder_never_observes_known_or_first_excess_response_bytes() {
    OVERFLOW_DECODER_CALLS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"four".to_vec()),
        ResponseScript::ok(b"known".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"four".to_vec(), b"first-excess-secret".to_vec()],
        },
    ])
    .expect("response limit server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    assert_eq!(
        codec_request::<Text, Text>(core.clone(), "four".to_owned())
            .await
            .expect("exact response limit"),
        "four"
    );
    for expected_path in ["known response preflight", "unknown response overflow"] {
        let error = codec_request::<Text, OverflowDecoder>(core.clone(), "input".to_owned())
            .await
            .expect_err(expected_path);
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("known"));
            assert!(!formatted.contains("first-excess-secret"));
        }
    }
    assert_eq!(OVERFLOW_DECODER_CALLS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn real_content_type_mismatch_rejects_before_response_body_polling() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"content-contract-body-secret-5dd4".to_vec(),
        gate: gate.clone(),
    }])
    .expect("content contract server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        codec_request::<Text, RejectingDecoder>(core, "input".to_owned()),
    )
    .await
    .expect("content validation must not wait for the body")
    .expect_err("missing content type");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("content-contract-body-secret-5dd4"));
    }
}

#[tokio::test]
async fn real_reqwest_stream_upload_transmits_exact_bytes_and_owned_media_type() {
    let server = ScriptedServer::start([ResponseScript::ok(b"upload-ok".to_vec())])
        .expect("stream upload server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"real-"),
            ProbeFrame::Data(b"stream-"),
            ProbeFrame::Data(b"upload"),
        ],
        &polls,
    ))
    .exact_length(18);
    let body = RequestBody::stream(stream)
        .content_type("application/x-concord-stream")
        .expect("valid stream content type");
    assert_eq!(
        body_request(core, body).await.expect("stream upload"),
        b"upload-ok"[..]
    );
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("recorded stream request");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].target(), "/v1/body/upload");
    assert_eq!(requests[0].body(), b"real-stream-upload");
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-concord-stream")
    );
    assert_eq!(requests[0].header("content-length"), Some("18"));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn real_reqwest_multipart_upload_owns_framing_and_supports_streams_and_factories() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"buffered-ok".to_vec()),
        ResponseScript::ok(b"streamed-ok".to_vec()),
        ResponseScript::ok(b"factory-ok".to_vec()),
    ])
    .expect("multipart upload server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let buffered = MultipartBody::new()
        .part(
            "text-field",
            MultipartPart::text("multipart-text-value-secret-017c")
                .expect("bounded multipart text"),
        )
        .expect("text part")
        .part(
            "file-field",
            MultipartPart::bytes(Bytes::from_static(b"multipart-bytes-value-secret-929d"))
                .expect("bounded multipart bytes")
                .file_name("upload-secret-name.txt")
                .expect("filename")
                .content_type("application/x-concord-part")
                .expect("part content type"),
        )
        .expect("bytes part");
    assert_eq!(
        body_request(core.clone(), RequestBody::multipart(buffered))
            .await
            .expect("buffered multipart upload"),
        b"buffered-ok"[..]
    );

    let polls = Arc::new(AtomicUsize::new(0));
    let streamed = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"multipart-streamed-"),
            ProbeFrame::Data(b"value-secret-62e7"),
        ],
        &polls,
    ));
    let streamed = MultipartBody::new()
        .part(
            "stream-field",
            MultipartPart::stream(streamed)
                .file_name("stream-secret-name.bin")
                .expect("stream filename")
                .content_type("application/octet-stream")
                .expect("stream content type"),
        )
        .expect("stream part");
    assert_eq!(
        body_request(core.clone(), RequestBody::multipart(streamed))
            .await
            .expect("streamed multipart upload"),
        b"streamed-ok"[..]
    );

    let factory_calls = Arc::new(AtomicUsize::new(0));
    assert_eq!(
        body_request(
            core,
            RequestBody::multipart_factory(ProbeMultipartFactory {
                create_calls: Arc::clone(&factory_calls),
                fail: false,
            }),
        )
        .await
        .expect("factory multipart upload"),
        b"factory-ok"[..]
    );

    server
        .wait_for_requests(3, Duration::from_secs(2))
        .expect("recorded multipart requests");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        let content_type = request
            .header("content-type")
            .expect("Reqwest multipart content type");
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("boundary parameter");
        assert!(!boundary.is_empty());
        assert!(
            request
                .body()
                .starts_with(format!("--{boundary}\r\n").as_bytes())
        );
    }
    let buffered_wire = String::from_utf8_lossy(requests[0].body());
    assert!(buffered_wire.contains("name=\"text-field\""));
    assert!(buffered_wire.contains("multipart-text-value-secret-017c"));
    assert!(buffered_wire.contains("multipart-bytes-value-secret-929d"));
    assert!(buffered_wire.contains("upload-secret-name.txt"));
    assert!(buffered_wire.contains("application/x-concord-part"));
    assert!(requests[0].header("content-length").is_some());

    let streamed_wire = String::from_utf8_lossy(requests[1].body());
    assert!(streamed_wire.contains("multipart-streamed-value-secret-62e7"));
    assert!(streamed_wire.contains("stream-secret-name.bin"));
    assert_eq!(requests[1].header("transfer-encoding"), Some("chunked"));
    assert_eq!(polls.load(Ordering::SeqCst), 3);

    let factory_wire = String::from_utf8_lossy(requests[2].body());
    assert!(factory_wire.contains("factory-multipart-value"));
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn managed_producer_failure_precedes_reqwest_transport_failure() {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("managed producer listener");
    let address = listener.local_addr().expect("managed producer address");
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("managed producer accept");
        std::future::pending::<()>().await;
    });
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let hooks = Arc::new(ProbeHooks::new());
    let core = ClientCore::build(
        &format!("http://{address}/v1/"),
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().hooks(hooks.clone()),
    )
    .expect("managed producer core");
    let polls = Arc::new(AtomicUsize::new(0));
    let body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Failure("native-producer-secret-88be")],
        &polls,
    ))
    .exact_length(1);
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        body_request(core, RequestBody::stream(body)),
    )
    .await
    .expect("managed producer failure should be prompt")
    .expect_err("managed producer request must fail");
    peer.abort();
    let _ = peer.await;
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("native-producer-secret-88be"));
}

#[tokio::test]
async fn cancellation_drops_direct_and_factory_created_request_streams_without_completion() {
    async fn cancel(body: RequestBody, reached: Arc<tokio::sync::Notify>) -> Arc<ProbeHooks> {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
        let task = tokio::spawn(async move { body_request(core, body).await });
        reached.notified().await;
        assert_eq!(executor.application().request_count().unwrap(), 0);
        task.abort();
        let join = task.await.expect_err("request task must be cancelled");
        assert!(join.is_cancelled());
        tokio::task::yield_now().await;
        assert_eq!(executor.application().request_count().unwrap(), 0);
        hooks
    }

    let direct_polls = Arc::new(AtomicUsize::new(0));
    let direct_dropped = Arc::new(AtomicBool::new(false));
    let direct_reached = Arc::new(tokio::sync::Notify::new());
    let direct_body = RequestBody::stream(StreamBody::from_stream(PendingBodyStream {
        polls: Arc::clone(&direct_polls),
        dropped: Arc::clone(&direct_dropped),
        reached: Arc::clone(&direct_reached),
    }));
    let direct_hooks = cancel(direct_body, Arc::clone(&direct_reached)).await;
    let direct_polls_at_drop = direct_polls.load(Ordering::SeqCst);
    assert!(direct_polls_at_drop > 0);
    assert!(direct_dropped.load(Ordering::SeqCst));
    assert_eq!(direct_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(direct_polls.load(Ordering::SeqCst), direct_polls_at_drop);

    let factory_calls = Arc::new(AtomicUsize::new(0));
    let factory_polls = Arc::new(AtomicUsize::new(0));
    let factory_dropped = Arc::new(AtomicBool::new(false));
    let factory_reached = Arc::new(tokio::sync::Notify::new());
    let factory_body = RequestBody::factory(PendingBodyFactory {
        create_calls: Arc::clone(&factory_calls),
        polls: Arc::clone(&factory_polls),
        dropped: Arc::clone(&factory_dropped),
        reached: Arc::clone(&factory_reached),
    });
    let factory_hooks = cancel(factory_body, Arc::clone(&factory_reached)).await;
    let factory_polls_at_drop = factory_polls.load(Ordering::SeqCst);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert!(factory_polls_at_drop > 0);
    assert!(factory_dropped.load(Ordering::SeqCst));
    assert_eq!(factory_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(factory_polls.load(Ordering::SeqCst), factory_polls_at_drop);

    let prepared_polls = Arc::new(AtomicUsize::new(0));
    let prepared_dropped = Arc::new(AtomicBool::new(false));
    let prepared_reached = Arc::new(tokio::sync::Notify::new());
    let prepared_executor = DeterministicExecutor::new();
    let prepared_hooks = Arc::new(ProbeHooks::new());
    let prepared_core = development_core(
        &prepared_executor,
        Arc::clone(&prepared_hooks),
        RuntimeConfigBuilder::new(),
    );
    let prepared_endpoint = RuntimeEndpoint::builder(
        EndpointId::from_static("runtime.cancellation"),
        Method::POST,
    )
    .path_segment("prepared-cancellation")
    .body(RequestBody::stream(StreamBody::from_stream(
        PendingBodyStream {
            polls: Arc::clone(&prepared_polls),
            dropped: Arc::clone(&prepared_dropped),
            reached: Arc::clone(&prepared_reached),
        },
    )))
    .response_bytes()
    .build()
    .expect("prepared cancellation endpoint");
    let prepared_task =
        tokio::spawn(async move { Request::new(prepared_core, prepared_endpoint).await });
    prepared_reached.notified().await;
    assert_eq!(prepared_executor.application().request_count().unwrap(), 0);
    prepared_task.abort();
    assert!(
        prepared_task
            .await
            .expect_err("prepared request must be cancelled")
            .is_cancelled()
    );
    tokio::task::yield_now().await;
    let prepared_polls_at_drop = prepared_polls.load(Ordering::SeqCst);
    assert!(prepared_polls_at_drop > 0);
    assert!(prepared_dropped.load(Ordering::SeqCst));
    assert_eq!(prepared_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(
        prepared_polls.load(Ordering::SeqCst),
        prepared_polls_at_drop
    );

    #[cfg(feature = "multipart")]
    {
        let multipart_polls = Arc::new(AtomicUsize::new(0));
        let multipart_dropped = Arc::new(AtomicBool::new(false));
        let multipart_reached = Arc::new(tokio::sync::Notify::new());
        let stream = StreamBody::from_stream(PendingBodyStream {
            polls: Arc::clone(&multipart_polls),
            dropped: Arc::clone(&multipart_dropped),
            reached: Arc::clone(&multipart_reached),
        });
        let multipart_body = RequestBody::multipart(
            MultipartBody::new()
                .part("stream", MultipartPart::stream(stream))
                .expect("multipart pending stream"),
        );
        let multipart_hooks = cancel(multipart_body, Arc::clone(&multipart_reached)).await;
        let multipart_polls_at_drop = multipart_polls.load(Ordering::SeqCst);
        assert!(multipart_polls_at_drop > 0);
        assert!(multipart_dropped.load(Ordering::SeqCst));
        assert_eq!(
            multipart_hooks.completion_returned.load(Ordering::SeqCst),
            0
        );
        tokio::task::yield_now().await;
        assert_eq!(
            multipart_polls.load(Ordering::SeqCst),
            multipart_polls_at_drop
        );
    }
}

#[tokio::test]
async fn real_reqwest_bytes_are_finite_complete_only_and_body_failures_are_safe() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"within".to_vec()),
        ResponseScript::ok(b"known-too-large".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"ab".to_vec(), b"cd".to_vec()],
        },
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"abcd".to_vec(), b"excess".to_vec()],
        },
        ResponseScript::Truncated {
            status: 200,
            declared_length: 6,
            prefix: b"short".to_vec(),
        },
    ])
    .expect("bytes server");
    let runtime = || RuntimeConfigBuilder::new().response_body_limit(NonZeroUsize::new(6).unwrap());
    let core = real_core(&server, Arc::new(ProbeHooks::new()), runtime());
    assert_eq!(
        bytes_request(core.clone()).await.expect("within"),
        b"within"[..]
    );
    assert_eq!(
        bytes_request(core.clone())
            .await
            .expect_err("known excessive length")
            .category(),
        ErrorCategory::ResponseBody
    );
    assert_eq!(
        bytes_request(core.clone()).await.expect("unknown within"),
        b"abcd"[..]
    );
    let overflow = bytes_request(core.clone())
        .await
        .expect_err("unknown overflow");
    assert_eq!(overflow.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{overflow:?} {overflow}").contains("abcd"));
    assert!(!format!("{overflow:?} {overflow}").contains("excess"));
    assert_eq!(
        bytes_request(core)
            .await
            .expect_err("native body failure")
            .category(),
        ErrorCategory::ResponseBody
    );
    assert_eq!(server.request_count(), 5);
}

#[tokio::test]
async fn real_reqwest_bytes_collection_is_bounded_by_operation_deadline() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"delayed body".to_vec(),
        gate: gate.clone(),
    }])
    .expect("gated bytes server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core)
        .timeout(Duration::from_millis(500))
        .await
        .expect_err("body deadline");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&hooks.records()).2, Some(StatusCode::OK));
}

#[tokio::test]
async fn no_content_accepts_every_2xx_without_polling_the_body() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedBody {
            status: 200,
            body: b"must-not-be-polled".to_vec(),
            gate: gate.clone(),
        },
        ResponseScript::status(204, Vec::new()),
        ResponseScript::status(205, b"ignored".to_vec()),
    ])
    .expect("no-content server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    tokio::time::timeout(Duration::from_secs(1), no_content_request(core.clone()))
        .await
        .expect("200 must not wait for body gate")
        .expect("200 no-content");
    gate.release();
    no_content_request(core.clone())
        .await
        .expect("204 no-content");
    let response = no_content_request(core)
        .response()
        .await
        .expect("205 metadata");
    assert_eq!(response.status(), StatusCode::RESET_CONTENT);
    assert_eq!(response.value(), &());
}

#[tokio::test]
async fn all_non_2xx_statuses_use_complete_only_bounded_terminal_capture() {
    let deadline_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::status(302, b"redirect".to_vec()),
        ResponseScript::status(404, b"missing".to_vec()),
        ResponseScript::status(500, b"known-too-large".to_vec()),
        ResponseScript::Chunked {
            status: 502,
            headers: Vec::new(),
            chunks: vec![b"prefix".to_vec(), b"overflow".to_vec()],
        },
        ResponseScript::Truncated {
            status: 503,
            declared_length: 7,
            prefix: b"short".to_vec(),
        },
        ResponseScript::GatedBody {
            status: 504,
            body: b"late".to_vec(),
            gate: deadline_gate.clone(),
        },
    ])
    .expect("status server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().status_body_limit(NonZeroUsize::new(8).unwrap()),
    );

    let redirect = bytes_request(core.clone()).await.expect_err("3xx terminal");
    assert_eq!(redirect.http_status(), Some(StatusCode::FOUND));
    assert_eq!(
        redirect.http_response().unwrap().body().state(),
        StatusBodyState::Complete
    );
    let missing = bytes_request(core.clone()).await.expect_err("4xx terminal");
    assert_eq!(missing.http_status(), Some(StatusCode::NOT_FOUND));
    assert_eq!(
        missing
            .http_response()
            .unwrap()
            .body()
            .bytes()
            .unwrap()
            .as_ref(),
        b"missing"
    );
    let known = bytes_request(core.clone()).await.expect_err("5xx terminal");
    assert_eq!(known.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(
        known.http_response().unwrap().body().state(),
        StatusBodyState::OmittedKnownTooLarge
    );
    let overflow = bytes_request(core.clone())
        .await
        .expect_err("unknown overflow");
    assert_eq!(overflow.http_status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(
        overflow.http_response().unwrap().body().state(),
        StatusBodyState::OmittedLimitExceeded
    );
    assert!(overflow.http_response().unwrap().body().bytes().is_none());
    let read = bytes_request(core.clone())
        .await
        .expect_err("status read failure");
    assert_eq!(read.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        read.http_response().unwrap().body().state(),
        StatusBodyState::UnavailableReadFailure
    );
    let deadline = bytes_request(core)
        .timeout(Duration::from_millis(500))
        .await
        .expect_err("status capture deadline");
    deadline_gate.release();
    assert_eq!(deadline.category(), ErrorCategory::HttpStatus);
    assert_eq!(deadline.http_status(), Some(StatusCode::GATEWAY_TIMEOUT));
    assert_eq!(
        deadline.http_response().unwrap().body().state(),
        StatusBodyState::UnavailableDeadline
    );
    let records = hooks.records();
    let completions = records
        .iter()
        .filter_map(|record| match record {
            LifecycleRecord::Complete {
                outcome, status, ..
            } => Some((*outcome, *status)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(completions.len(), 6);
    assert!(
        completions
            .iter()
            .all(|(outcome, _)| *outcome == CompletionOutcome::HttpStatus)
    );
    assert_eq!(
        completions
            .iter()
            .map(|(_, status)| *status)
            .collect::<Vec<_>>(),
        [
            Some(StatusCode::FOUND),
            Some(StatusCode::NOT_FOUND),
            Some(StatusCode::INTERNAL_SERVER_ERROR),
            Some(StatusCode::BAD_GATEWAY),
            Some(StatusCode::SERVICE_UNAVAILABLE),
            Some(StatusCode::GATEWAY_TIMEOUT),
        ]
    );
}

#[test]
fn public_associated_futures_are_named_sendable_box_futures() {
    fn assert_box_future<T>(_: BoxFuture<'static, T>) {}

    let executor = DeterministicExecutor::new();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    assert_box_future(bytes_request(core.clone()).into_future());
    assert_box_future(bytes_request(core).response().into_future());
}

impl fmt::Debug for ProbeHooks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProbeHooks")
            .field(
                "record_count",
                &self.records.lock().map(|records| records.len()).ok(),
            )
            .field("veto", &self.veto.load(Ordering::SeqCst))
            .field(
                "completion_returned",
                &self.completion_returned.load(Ordering::SeqCst),
            )
            .finish()
    }
}
