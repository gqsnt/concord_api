use super::*;

#[tokio::test]
async fn synchronous_attempt_observation_consumes_the_operation_deadline() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks {
        attempt_delay: Some(Duration::from_millis(20)),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core)
        .timeout(Duration::from_millis(1))
        .await
        .expect_err("observer time is inside the deadline");

    let records = hooks.records();
    let before_id = match records.first() {
        Some(LifecycleRecord::Before { operation_id, .. }) => *operation_id,
        _ => panic!("before-send record"),
    };
    let (completion_id, outcome, status, elapsed) = completion(&records);
    assert_eq!(completion_id, before_id);
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(status, None);
    assert!(elapsed >= Duration::from_millis(20));

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
async fn operation_observer_runs_once_and_has_no_result_authority() {
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
async fn delayed_polling_consumes_no_budget_and_every_operation_has_a_deadline() {
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

    let finite_executor = DeterministicExecutor::new();
    let finite_gate = Gate::closed();
    finite_executor
        .application()
        .pause_before_execution(finite_gate.clone())
        .expect("finite gate");
    finite_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")))
        .expect("response");
    let finite_core = development_core(
        &finite_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let task = tokio::spawn(async move {
        bytes_request(finite_core)
            .timeout(Duration::from_secs(1))
            .await
    });
    finite_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(100)).await;
    let error = task.await.expect("join").expect_err("deadline must expire");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    finite_gate.open();
}
