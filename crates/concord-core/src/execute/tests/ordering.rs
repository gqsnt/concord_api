use super::*;

#[tokio::test]
async fn limiter_observes_before_user_response_hook() {
    const KEY_SENTINEL: &str = "controlled-custom-key-5e8c7f2b";
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"limited"))
                .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("limited response");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let observe_gate = Gate::closed();
    let limiter = Arc::new(ProbeRateLimiter {
        trace: Some(Arc::clone(&trace)),
        observe_gate: Some(observe_gate.clone()),
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
            .rate_limit_cooldown_cap(Duration::from_secs(7))
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));
    let request = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), KEY_SENTINEL);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let task = tokio::spawn(request.into_future());
    observe_gate.wait_until_reached().await;
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(
        trace.lock().expect("trace").as_slice(),
        ["before_send", "limiter acquire", "limiter observe"]
    );
    observe_gate.open();
    assert_eq!(
        task.await.expect("request task").expect("limited result"),
        Bytes::from_static(b"limited")
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(
        trace.lock().expect("trace").as_slice(),
        [
            "before_send",
            "limiter acquire",
            "limiter observe",
            "on_response_head",
            "on_complete",
        ]
    );
    assert_eq!(
        limiter
            .response_cooldown_caps
            .lock()
            .expect("response cooldown caps")
            .as_slice(),
        [Duration::from_secs(7)]
    );
    let records = limiter.request_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].endpoint, ENDPOINT);
    assert_eq!(records[0].method, Method::GET);
    assert_eq!(
        records[0].safe_url,
        "http://execution.example.test/v1/health/bytes"
    );
    assert_eq!(records[0].host.as_deref(), Some("execution.example.test"));
    assert_eq!(records[0].page_index, 0);
    assert_eq!(records[0].rule_ids, ["ExecutionApi::rate_limit::tenant"]);
    assert_eq!(records[0].key_values, [KEY_SENTINEL]);
    let ordinary_diagnostics = format!(
        "{}\n{:?}\n{:?}",
        debug.lock().expect("debug output").join("\n"),
        hooks.records(),
        trace.lock().expect("trace"),
    );
    assert!(
        !ordinary_diagnostics.contains(KEY_SENTINEL),
        "resolved keys are visible only in the controlled custom-limiter capture"
    );
}

#[tokio::test]
async fn limiter_learns_cooldown_when_user_hook_would_fail() {
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::RETRY_AFTER,
        http::HeaderValue::from_static("5"),
    );
    executor
        .application()
        .queue_response(
            ScriptedResponse::status(
                StatusCode::TOO_MANY_REQUESTS,
                Bytes::from_static(b"withheld"),
            )
            .headers(headers)
            .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("cooldown response");
    let hooks = Arc::new(ProbeHooks {
        response_fail: true,
        ..ProbeHooks::new()
    });
    let limiter = Arc::new(ProbeRateLimiter {
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );

    let error = bytes_request(core)
        .await
        .expect_err("response hook rejection");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert!(limiter.learned_cooldown.load(Ordering::SeqCst));
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        hooks.records().as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Response(_, StatusCode::TOO_MANY_REQUESTS),
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Hook,
                status: Some(StatusCode::TOO_MANY_REQUESTS),
                ..
            }
        ]
    ));
}

#[tokio::test]
async fn limiter_observation_error_prevents_user_hook() {
    const RESPONSE_SECRET: &str = "response-observation-secret-4f6a";
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_static(RESPONSE_SECRET),
    );
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"withheld"))
                .headers(headers)
                .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("observation-error response");
    let hooks = Arc::new(ProbeHooks {
        response_fail: true,
        ..ProbeHooks::new()
    });
    let limiter = Arc::new(ProbeRateLimiter {
        response_body_polls: Some(Arc::clone(&body_polls)),
        observe_fail: true,
        ..ProbeRateLimiter::new()
    });
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );

    let error = bytes_request(core)
        .await
        .expect_err("limiter observation failure");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::Observe)
    );
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        hooks.records().as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::RateLimit,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(RESPONSE_SECRET));
        assert!(!format!("{current:?}").contains(RESPONSE_SECRET));
        source = current.source();
    }
}

#[tokio::test]
async fn limiter_observation_uses_sanitized_head() {
    const RESPONSE_SECRET: &str = "response-header-secret-a31d";
    const PUBLIC_VALUE: &str = "public-response-marker-920b";
    let executor = DeterministicExecutor::new();
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        http::HeaderValue::from_static(RESPONSE_SECRET),
    );
    headers.insert(
        http::HeaderName::from_static("x-observation-marker"),
        http::HeaderValue::from_static(PUBLIC_VALUE),
    );
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"sanitized")).headers(headers),
        )
        .expect("sanitized response");
    let limiter = Arc::new(ProbeRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );

    assert_eq!(
        bytes_request(core).await.expect("sanitized response"),
        Bytes::from_static(b"sanitized")
    );
    let responses = limiter.response_records();
    assert_eq!(responses.len(), 1);
    assert_eq!(
        responses[0].safe_url,
        "http://execution.example.test/v1/health/bytes"
    );
    assert_eq!(responses[0].status, StatusCode::OK);
    assert!(responses[0].headers.contains("authorization"));
    assert!(responses[0].headers.contains("Redacted"));
    assert!(responses[0].headers.contains(PUBLIC_VALUE));
    assert!(!responses[0].headers.contains(RESPONSE_SECRET));
}

#[tokio::test]
async fn response_bytes_reuses_limiter_hooks_observation_and_success_completion() {
    const RAW_BODY: &[u8] = b"raw-lifecycle-body-secret-2f5e";
    const RATE_KEY: &str = "raw-lifecycle-rate-key-secret-e3b1";
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(RAW_BODY)))
        .expect("raw lifecycle response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
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

    let response = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), RATE_KEY)
        .response_bytes()
        .await
        .expect("response-bytes lifecycle success");
    assert_eq!(response.value(), RAW_BODY);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.url().as_str(),
        "http://execution.example.test/v1/health/bytes"
    );
    assert_eq!(response.metadata().endpoint(), ENDPOINT);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert!(matches!(
        hooks.records().as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Response(_, StatusCode::OK),
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Success,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
    let automatic = format!(
        "{}\n{:?}\n{response:?}",
        debug.lock().expect("debug output").join("\n"),
        hooks.records(),
    );
    assert!(!automatic.contains("raw-lifecycle-body-secret-2f5e"));
    assert!(!automatic.contains(RATE_KEY));
}

#[tokio::test]
async fn limiter_failure_prevents_request_debug_and_native_execution() {
    const KEY_SENTINEL: &str = "limiter-failure-key-private-73cf9a0d";
    let executor = DeterministicExecutor::new();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let limiter = Arc::new(ProbeRateLimiter {
        fail: true,
        trace: Some(Arc::clone(&trace)),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = development_core(
        &executor,
        hooks,
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), KEY_SENTINEL)
        .await
        .expect_err("limiter rejection");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::Acquire)
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{current:?}").contains(KEY_SENTINEL));
        source = current.source();
    }
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
    assert_eq!(
        trace.lock().expect("trace").as_slice(),
        ["before_send", "limiter acquire", "on_complete"]
    );
}

#[tokio::test(start_paused = true)]
async fn original_deadline_cancels_limiter_wait_without_native_send() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter {
        acquire_gate: Some(gate.clone()),
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = development_core(
        &executor,
        hooks,
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let task = tokio::spawn(
        rate_limited_bytes_request(core, Some(Duration::from_secs(5)), "limited").into_future(),
    );
    gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = task
        .await
        .expect("request task")
        .expect_err("deadline timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn prepared_plans_use_selected_limiter_metadata_and_configured_key_limit() {
    fn rule(id: &str, key: RateLimitKey) -> RateLimitPolicy {
        RateLimitPolicy::new(
            RateLimitId::new(id).expect("prepared limiter policy ID"),
            key,
            NonZeroU32::new(1).expect("prepared limiter cost"),
            [RateLimitWindow::new(
                NonZeroU32::new(10).expect("prepared limiter capacity"),
                Duration::from_secs(1),
            )
            .expect("prepared limiter window")],
        )
        .expect("prepared limiter rule")
    }

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"governed")))
        .expect("prepared limiter response");
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.governed"), Method::GET)
            .path_segment("governed")
            .rate_limit(rule("Prepared::governed", RateLimitKey::global()))
            .response_bytes()
            .build()
            .expect("prepared governed endpoint");
    assert_eq!(
        Request::new(core, endpoint)
            .await
            .expect("prepared limiter execution"),
        Bytes::from_static(b"governed")
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);

    let rejected_executor = DeterministicExecutor::new();
    let restricted = Arc::new(ProbeRateLimiter {
        key_limit: 3,
        ..ProbeRateLimiter::new()
    });
    let configured: Arc<dyn RateLimiter> = restricted.clone();
    let rejected_core = development_core(
        &rejected_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let rejected =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.key_limit"), Method::GET)
            .rate_limit(rule(
                "Prepared::key_limit",
                RateLimitKey::from_parts([
                    RateLimitKeyPart::new("tenant", "sensitive-key").expect("tenant key part")
                ])
                .expect("prepared oversized key"),
            ))
            .response_bytes()
            .build()
            .expect("prepared oversized-plan endpoint");
    let error = Request::new(rejected_core, rejected)
        .await
        .expect_err("prepared key limit must fail during preparation");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(restricted.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(
        rejected_executor
            .application()
            .request_count()
            .expect("rejected count"),
        0
    );
}

#[tokio::test]
async fn hook_body_and_key_preparation_failures_perform_no_limiter_work() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    hooks.veto.store(true, Ordering::SeqCst);
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(configured),
    );
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), "hook-veto-key")
        .await
        .expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);

    hooks.veto.store(false, Ordering::SeqCst);
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let error = body_request(
        core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::new(AtomicUsize::new(0)),
            create_calls: Arc::new(AtomicUsize::new(0)),
            polls: Arc::new(AtomicUsize::new(0)),
            fail: true,
        }),
    )
    .await
    .expect_err("body failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);

    let hook_records_before_key = hooks.records().len();
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), "x".repeat(1_025))
        .await
        .expect_err("key bound");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(
        hooks.records().len(),
        hook_records_before_key + 1,
        "key rejection occurs before before-send; only completion is added"
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn default_key_limit_accepts_1024_bytes_and_rejects_the_first_excess() {
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"boundary")))
        .expect("boundary response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );

    let fitting = key_value_with_len(1_024 - KEY_PART_NAME_BYTES, "default-fit-");
    assert_eq!(fitting.len() + KEY_PART_NAME_BYTES, 1_024);
    assert_eq!(
        rate_limited_bytes_request(core.clone(), Some(Duration::from_secs(30)), fitting)
            .await
            .expect("exact default boundary"),
        Bytes::from_static(b"boundary")
    );

    let excess = key_value_with_len(1_025 - KEY_PART_NAME_BYTES, "default-excess-");
    let error = rate_limited_bytes_request(core, Some(Duration::from_secs(30)), excess)
        .await
        .expect_err("first excess must fail during preparation");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1,
        "only the fitting request reaches before-send"
    );
}

#[tokio::test]
async fn configured_key_limit_enforces_lower_zero_and_increased_boundaries() {
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();

    let lower_executor = DeterministicExecutor::new();
    lower_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"lower")))
        .expect("lower response");
    let lower = Arc::new(ProbeRateLimiter {
        key_limit: 16,
        ..ProbeRateLimiter::new()
    });
    let lower_object: Arc<dyn RateLimiter> = lower.clone();
    let lower_core = development_core(
        &lower_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(lower_object),
    );
    let fitting = key_value_with_len(16 - KEY_PART_NAME_BYTES, "fit16");
    assert_eq!(
        rate_limited_bytes_request(lower_core.clone(), Some(Duration::from_secs(30)), fitting)
            .await
            .expect("configured lower boundary"),
        Bytes::from_static(b"lower")
    );
    let error = rate_limited_bytes_request(
        lower_core,
        Some(Duration::from_secs(30)),
        key_value_with_len(17 - KEY_PART_NAME_BYTES, "over16"),
    )
    .await
    .expect_err("first excess over lower limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(lower.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        lower_executor.application().request_count().expect("count"),
        1
    );

    let zero_executor = DeterministicExecutor::new();
    zero_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"global")))
        .expect("global response");
    let zero = Arc::new(ProbeRateLimiter {
        key_limit: 0,
        ..ProbeRateLimiter::new()
    });
    let zero_object: Arc<dyn RateLimiter> = zero.clone();
    let zero_core = development_core(
        &zero_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(zero_object),
    );
    assert_eq!(
        Request::new(zero_core.clone(), GlobalRateLimitedBytesEndpoint)
            .await
            .expect("global key under zero limit"),
        Bytes::from_static(b"global")
    );
    let error = rate_limited_bytes_request(zero_core, Some(Duration::from_secs(30)), String::new())
        .await
        .expect_err("nonempty key structure exceeds zero even with an empty value");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(zero.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        zero_executor.application().request_count().expect("count"),
        1
    );

    let increased_executor = DeterministicExecutor::new();
    increased_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"increased")))
        .expect("increased response");
    let increased = Arc::new(ProbeRateLimiter {
        key_limit: 2_048,
        ..ProbeRateLimiter::new()
    });
    let increased_object: Arc<dyn RateLimiter> = increased.clone();
    let increased_core = development_core(
        &increased_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(increased_object),
    );
    let fitting = key_value_with_len(2_048 - KEY_PART_NAME_BYTES, "increased-fit-");
    assert!(fitting.len() + KEY_PART_NAME_BYTES > 1_024);
    assert_eq!(
        rate_limited_bytes_request(
            increased_core.clone(),
            Some(Duration::from_secs(30)),
            fitting,
        )
        .await
        .expect("key above the default but within configured limit"),
        Bytes::from_static(b"increased")
    );
    let error = rate_limited_bytes_request(
        increased_core,
        Some(Duration::from_secs(30)),
        key_value_with_len(2_049 - KEY_PART_NAME_BYTES, "increased-excess-"),
    )
    .await
    .expect_err("first excess over increased limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(increased.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        increased_executor
            .application()
            .request_count()
            .expect("count"),
        1
    );
}

#[tokio::test]
async fn configured_key_limit_snapshot_is_shared_by_clones_and_retained_by_pending_requests() {
    const KEY_PART_NAME_BYTES: usize = "vars.tenant".len();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"clone")))
        .expect("clone response");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"independent",
        )))
        .expect("independent response");

    let configured = Arc::new(ProbeRateLimiter {
        key_limit: 16,
        ..ProbeRateLimiter::new()
    });
    let configured_object: Arc<dyn RateLimiter> = configured.clone();
    let configured_core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::clone(&configured_object)),
    );
    let configured_clone = configured_core.clone();
    assert!(Arc::ptr_eq(
        configured_core.rate_limiter(),
        configured_clone.rate_limiter()
    ));
    assert_eq!(configured_core.rate_limit_key_limit(), 16);
    assert_eq!(configured_clone.rate_limit_key_limit(), 16);

    let pending = rate_limited_bytes_request(
        configured_core,
        Some(Duration::from_secs(30)),
        key_value_with_len(17 - KEY_PART_NAME_BYTES, "pend17"),
    );
    assert_eq!(
        rate_limited_bytes_request(
            configured_clone,
            Some(Duration::from_secs(30)),
            key_value_with_len(16 - KEY_PART_NAME_BYTES, "clone"),
        )
        .await
        .expect("clone uses configured limit"),
        Bytes::from_static(b"clone")
    );

    let independent_limiter = Arc::new(ProbeRateLimiter::new());
    let independent = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().rate_limiter(independent_limiter),
    );
    assert_eq!(independent.rate_limit_key_limit(), 1_024);
    assert!(!Arc::ptr_eq(independent.rate_limiter(), &configured_object));
    assert_eq!(
        rate_limited_bytes_request(
            independent,
            Some(Duration::from_secs(30)),
            key_value_with_len(17 - KEY_PART_NAME_BYTES, "def017"),
        )
        .await
        .expect("independent default build retains default limit"),
        Bytes::from_static(b"independent")
    );

    let error = pending
        .await
        .expect_err("pending request retains originating configured limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(configured.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn private_fallback_rejects_a_nonempty_runtime_plan_before_send() {
    let executor = DeterministicExecutor::new();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = rate_limited_bytes_request(core.clone(), Some(Duration::from_secs(30)), "nonempty")
        .await
        .expect_err("fallback must reject a nonempty runtime plan");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::InvalidConfiguration)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let rule = RateLimitPolicy::new(
        RateLimitId::new("Prepared::fallback").expect("prepared fallback ID"),
        RateLimitKey::global(),
        NonZeroU32::new(1).expect("prepared fallback cost"),
        [RateLimitWindow::new(
            NonZeroU32::new(1).expect("prepared fallback capacity"),
            Duration::from_secs(1),
        )
        .expect("prepared fallback window")],
    )
    .expect("prepared fallback rule");
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.fallback"), Method::GET)
            .rate_limit(rule)
            .response_bytes()
            .build()
            .expect("prepared fallback endpoint");
    let error = Request::new(core, endpoint)
        .await
        .expect_err("fallback must reject a prepared nonempty plan");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::InvalidConfiguration)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn private_protocol_recovery_does_not_reenter_custom_limiter_or_lifecycle() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new().http2_prior_knowledge(),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(configured),
    )
    .expect("real HTTP/2 core");
    assert_eq!(
        bytes_request(core)
            .await
            .expect("private protocol recovery"),
        Bytes::new()
    );
    server.finish().await;
    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    let records = hooks.records();
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Response(_, _)))
            .count(),
        1
    );

    let prepared_server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let prepared_physical_attempts = prepared_server.physical_attempts();
    let prepared_hooks = Arc::new(ProbeHooks::new());
    let prepared_limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = prepared_limiter.clone();
    let prepared_core = ClientCore::build(
        &prepared_server.base_url(),
        HttpConfigBuilder::new().http2_prior_knowledge(),
        RuntimeConfigBuilder::new()
            .hooks(prepared_hooks.clone())
            .rate_limiter(configured),
    )
    .expect("prepared real HTTP/2 core");
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.hidden_retry"), Method::GET)
            .response_bytes()
            .build()
            .expect("prepared protocol-recovery endpoint");
    assert_eq!(
        Request::new(prepared_core, endpoint)
            .await
            .expect("prepared private protocol recovery"),
        Bytes::new()
    );
    prepared_server.finish().await;
    assert_eq!(prepared_physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(prepared_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(prepared_limiter.observations.load(Ordering::SeqCst), 1);
    let prepared_records = prepared_hooks.records();
    assert_eq!(
        prepared_records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1
    );
    assert_eq!(
        prepared_records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Response(_, _)))
            .count(),
        1
    );
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn response_bytes_contains_protocol_recovery_inside_one_logical_attempt() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new().http2_prior_knowledge(),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(configured),
    )
    .expect("response-bytes HTTP/2 core");

    let response = bytes_request(core)
        .response_bytes()
        .await
        .expect("response-bytes private protocol recovery");
    assert!(response.value().is_empty());
    server.finish().await;
    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    let records = hooks.records();
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
            .count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Response(_, _)))
            .count(),
        1
    );
    assert_eq!(completion(&records).1, CompletionOutcome::Success);
}

#[tokio::test]
async fn direct_and_metadata_terminals_use_one_typed_managed_path() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let expected_url = "http://execution.example.test/v1/health/bytes";
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::GET)
                .safe_url(expected_url)
                .body(Bytes::new()),
            ScriptedResponse::success(Bytes::from_static(b"direct")),
        )
        .expect("direct script");
    assert_eq!(
        bytes_request(core.clone()).await.expect("direct bytes"),
        b"direct"[..]
    );

    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"metadata")))
        .expect("metadata script");
    let response = bytes_request(core)
        .response()
        .await
        .expect("metadata response");
    assert_eq!(response.value(), &b"metadata"[..]);
    assert_eq!(response.url().as_str(), expected_url);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());

    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].body_category(), BodyCategory::Empty);
    assert_eq!(captures[0].known_body_length(), Some(0));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
}
