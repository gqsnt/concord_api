use super::*;

#[tokio::test(start_paused = true)]
async fn timeout_in_request_gate_is_fixed_before_synchronous_completion_observation() {
    let executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let task =
        tokio::spawn(async move { bytes_request(core).timeout(Duration::from_secs(5)).await });

    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = task.await.expect("join").expect_err("fixed timeout");

    let records = hooks.records();
    let before_id = match records.first() {
        Some(LifecycleRecord::Before { operation_id, .. }) => *operation_id,
        _ => panic!("before-send record"),
    };
    let (completion_id, outcome, status, elapsed) = completion(&records);
    assert_eq!(completion_id, before_id);
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(status, None);
    assert_eq!(elapsed, Duration::from_secs(5));

    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn prepared_builder_deadline_is_fixed_before_completion_observation() {
    let executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.deadline"), Method::GET)
            .path("prepared-deadline")
            .timeout(Duration::from_secs(5))
            .accepted_bytes()
            .build()
            .expect("prepared deadline endpoint");
    let task = tokio::spawn(async move { Request::new(core, endpoint).await });

    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = task
        .await
        .expect("prepared deadline join")
        .expect_err("prepared deadline timeout");
    let records = hooks.records();
    let before_id = match records.first() {
        Some(LifecycleRecord::Before { operation_id, .. }) => *operation_id,
        _ => panic!("prepared before-send record"),
    };
    let (completion_id, outcome, status, elapsed) = completion(&records);
    assert_eq!(completion_id, before_id);
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(status, None);
    assert_eq!(elapsed, Duration::from_secs(5));

    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn synchronous_completion_observation_cannot_transform_success_or_status() {
    let success_executor = DeterministicExecutor::new();
    success_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"fixed")))
        .expect("success script");
    let success_hooks = Arc::new(ProbeHooks::new());
    let success_core = development_core(
        &success_executor,
        Arc::clone(&success_hooks),
        RuntimeConfigBuilder::new(),
    );
    assert_eq!(
        bytes_request(success_core)
            .timeout(Duration::from_secs(1))
            .await
            .expect("fixed success"),
        b"fixed"[..]
    );
    assert_eq!(
        completion(&success_hooks.records()).1,
        CompletionOutcome::Success
    );

    let status_executor = DeterministicExecutor::new();
    status_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"status"),
        ))
        .expect("status script");
    let status_hooks = Arc::new(ProbeHooks::new());
    let status_core = development_core(
        &status_executor,
        Arc::clone(&status_hooks),
        RuntimeConfigBuilder::new(),
    );
    let error = bytes_request(status_core)
        .timeout(Duration::from_secs(1))
        .await
        .expect_err("fixed status");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        completion(&status_hooks.records()).1,
        CompletionOutcome::HttpStatus
    );
}

#[tokio::test]
async fn completion_observer_runs_once_and_has_no_result_authority() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    assert_eq!(bytes_request(core).await.expect("success"), Bytes::new());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.records().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn accepted_bytes_keeps_the_fixed_deadline_and_cancellation_contract() {
    let timeout_executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let timeout_hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        ..ProbeHooks::new()
    });
    let timeout_core = development_core(
        &timeout_executor,
        Arc::clone(&timeout_hooks),
        RuntimeConfigBuilder::new(),
    );
    let timeout_task = tokio::spawn(async move {
        bytes_request(timeout_core)
            .timeout(Duration::from_secs(5))
            .accepted_bytes()
            .await
    });
    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let error = timeout_task
        .await
        .expect("accepted-bytes timeout join")
        .expect_err("accepted-bytes fixed timeout");
    let fixed_elapsed = completion(&timeout_hooks.records()).3;
    assert_eq!(fixed_elapsed, Duration::from_secs(5));
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(
        timeout_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );

    let cancellation_executor = DeterministicExecutor::new();
    cancellation_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"fixed raw")))
        .expect("response-bytes cancellation response");
    let cancellation_gate = Gate::closed();
    cancellation_executor
        .application()
        .pause_before_execution(cancellation_gate.clone())
        .expect("accepted-bytes cancellation gate");
    let cancellation_hooks = Arc::new(ProbeHooks::new());
    let cancellation_core = development_core(
        &cancellation_executor,
        Arc::clone(&cancellation_hooks),
        RuntimeConfigBuilder::new(),
    );
    let cancellation_task =
        tokio::spawn(async move { bytes_request(cancellation_core).accepted_bytes().await });
    cancellation_gate.wait_until_reached().await;
    cancellation_task.abort();
    assert!(
        cancellation_task
            .await
            .expect_err("response-bytes request task must be cancelled")
            .is_cancelled()
    );
    assert_eq!(
        cancellation_hooks
            .completion_returned
            .load(Ordering::SeqCst),
        0
    );
    cancellation_gate.open();
}

#[tokio::test(start_paused = true)]
async fn one_deadline_bounds_native_execution_and_response_head_hook() {
    let native_executor = DeterministicExecutor::new();
    let native_gate = Gate::closed();
    native_executor
        .application()
        .pause_before_execution(native_gate.clone())
        .expect("native gate");
    native_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("native response");
    let native_hooks = Arc::new(ProbeHooks::new());
    let native_core = development_core(
        &native_executor,
        Arc::clone(&native_hooks),
        RuntimeConfigBuilder::new(),
    );
    let native_task = tokio::spawn(async move {
        bytes_request(native_core)
            .timeout(Duration::from_secs(2))
            .await
    });
    native_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = native_task
        .await
        .expect("join")
        .expect_err("native timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&native_hooks.records()).2, None);

    let response_executor = DeterministicExecutor::new();
    response_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let response_gate = Gate::closed();
    let response_hooks = Arc::new(ProbeHooks {
        response_gate: Some(response_gate.clone()),
        ..ProbeHooks::new()
    });
    let response_core = development_core(
        &response_executor,
        Arc::clone(&response_hooks),
        RuntimeConfigBuilder::new(),
    );
    let response_task = tokio::spawn(async move {
        bytes_request(response_core)
            .timeout(Duration::from_secs(2))
            .await
    });
    response_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = response_task
        .await
        .expect("join")
        .expect_err("response hook timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    let records = response_hooks.records();
    let before_id = match records[0] {
        LifecycleRecord::Before { operation_id, .. } => operation_id,
        _ => panic!("before-send record"),
    };
    let response_id = match records[1] {
        LifecycleRecord::Response(id, StatusCode::OK) => id,
        _ => panic!("response-head record"),
    };
    assert_eq!(response_id, before_id);
    assert_eq!(completion(&records).0, before_id);
    assert_eq!(completion(&records).2, Some(StatusCode::OK));
}

#[tokio::test(start_paused = true)]
async fn limiter_observation_timeout_is_operation_error() {
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"withheld"))
                .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("limiter-timeout response");
    let observe_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter {
        observe_gate: Some(observe_gate.clone()),
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let task =
        tokio::spawn(async move { bytes_request(core).timeout(Duration::from_secs(2)).await });

    observe_gate.wait_until_reached().await;
    assert!(matches!(
        hooks.records().as_slice(),
        [LifecycleRecord::Before { .. }]
    ));
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = task
        .await
        .expect("limiter-timeout join")
        .expect_err("limiter observation timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        hooks.records().as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Timeout,
                status: Some(StatusCode::OK),
                ..
            }
        ]
    ));
}

#[tokio::test(start_paused = true)]
async fn delayed_polling_consumes_no_budget_and_disable_timeout_is_distinct() {
    let delayed_executor = DeterministicExecutor::new();
    delayed_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"late poll")))
        .expect("response");
    let delayed_core = development_core(
        &delayed_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let request = Request::new(
        delayed_core,
        BytesEndpoint::new(Some(Duration::from_secs(1))),
    );
    tokio::time::advance(Duration::from_secs(100)).await;
    assert_eq!(
        request.await.expect("deadline begins on poll"),
        b"late poll"[..]
    );

    let disabled_executor = DeterministicExecutor::new();
    let disabled_gate = Gate::closed();
    disabled_executor
        .application()
        .pause_before_execution(disabled_gate.clone())
        .expect("disabled gate");
    disabled_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"disabled")))
        .expect("response");
    let disabled_core = development_core(
        &disabled_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let task = tokio::spawn(async move { bytes_request(disabled_core).disable_timeout().await });
    disabled_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(100)).await;
    assert!(!task.is_finished());
    disabled_gate.open();
    assert_eq!(
        task.await.expect("join").expect("disabled timeout"),
        b"disabled"[..]
    );
}
