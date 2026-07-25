use super::*;

#[tokio::test]
async fn ordering_crosses_preparation_hooks_debug_transport_response_and_completion_once() {
    let executor = DeterministicExecutor::new();
    let transport_gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(transport_gate.clone())
        .expect("transport gate");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"ordered")))
        .expect("ordered response");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new())
        .with_debug_output(Arc::clone(&trace));
    let request =
        Request::new(core, BytesEndpoint::traced(Arc::clone(&trace))).debug(DebugLevel::Headers);
    let task = tokio::spawn(async move { request.await });

    transport_gate.wait_until_reached().await;
    let before_transport = trace.lock().expect("trace lock").clone();
    assert_eq!(before_transport[0], "head preparation");
    assert_eq!(before_transport[1], "before_send");
    assert!(before_transport[2].starts_with("request-ready"));
    assert_eq!(executor.application().request_count().expect("count"), 0);

    transport_gate.open();
    assert_eq!(
        task.await.expect("join").expect("ordered bytes"),
        b"ordered"[..]
    );
    let trace = trace.lock().expect("trace lock");
    assert!(trace[3].starts_with("response-head"));
    assert_eq!(trace[4], "on_response_head");
    assert!(trace[5].starts_with("completion"));
    assert_eq!(trace[6], "on_complete");
    assert_eq!(trace.len(), 7);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn before_send_veto_prevents_native_execution_and_later_response_phases() {
    let executor = DeterministicExecutor::new();
    let mut probe = ProbeHooks::new();
    probe.veto = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core).await.expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Hook,
                status: None,
                ..
            }
        ]
    ));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn completion_observes_fixed_limiter_failure() {
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"withheld"))
                .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("limiter-failure response");
    let completion_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let limiter = Arc::new(ProbeRateLimiter {
        observe_fail: true,
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let task = tokio::spawn(async move { bytes_request(core).await });

    completion_gate.wait_until_reached().await;
    assert!(!task.is_finished());
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
    completion_gate.open();
    let error = task
        .await
        .expect("limiter-failure join")
        .expect_err("fixed limiter failure");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::Observe)
    );
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}
