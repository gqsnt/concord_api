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
    assert_eq!(before_transport[1], "attempt_started");
    assert!(before_transport[2].starts_with("request-ready"));
    assert_eq!(executor.application().request_count().expect("count"), 0);

    transport_gate.open();
    assert_eq!(
        task.await.expect("join").expect("ordered bytes"),
        b"ordered"[..]
    );
    let trace = trace.lock().expect("trace lock");
    assert!(trace[3].starts_with("response-head"));
    assert_eq!(trace[4], "response_head_observed");
    assert!(trace[5].starts_with("completion"));
    assert_eq!(trace[6], "operation_finished");
    assert_eq!(trace.len(), 7);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn observer_panic_is_contained_and_cannot_change_success() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"observed")))
        .expect("observer panic response");
    let mut probe = ProbeHooks::new();
    probe.panic_on_attempt = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    assert_eq!(
        bytes_request(core).await.expect("fixed success"),
        b"observed"[..]
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
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
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn completion_observes_result_after_nonfallible_limiter_feedback() {
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"withheld"))
                .body_poll_counter(Arc::clone(&body_polls)),
        )
        .expect("limiter-failure response");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter {
        response_body_polls: Some(Arc::clone(&body_polls)),
        ..ProbeRateLimiter::new()
    });
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
    );
    let output = bytes_request(core)
        .await
        .expect("successful current result");
    assert_eq!(output, Bytes::from_static(b"withheld"));
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    assert!(body_polls.load(Ordering::SeqCst) >= 1);
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
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}
