use super::*;

#[tokio::test]
async fn pagination_pages_use_the_declared_status_acceptance_policy() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::NOT_FOUND,
            Bytes::from_static(b"accepted-page-item|-"),
        ))
        .expect("accepted pagination response");
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        |runtime| runtime,
    );
    let endpoint = AcceptedPageEndpoint(PageEndpoint::get(
        Some(Duration::from_secs(30)),
        CursorController::new("initial"),
    ));

    let items = paginate_with(Request::new(core, endpoint), PageLimit::Prefix(1))
        .await
        .expect("accepted-status pagination page");
    assert_eq!(items, ["accepted-page-item"]);
    assert_eq!(
        limiter
            .observations
            .lock()
            .expect("observation lock")
            .as_slice(),
        [(0, StatusCode::NOT_FOUND)]
    );
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 1);
    let aggregate = hooks.aggregate.lock().expect("aggregate lock");
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(aggregate[0].final_status, Some(StatusCode::NOT_FOUND));
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Success);
    assert_eq!(aggregate[0].attempts_started, 1);
    assert_eq!(aggregate[0].policy_retries, 0);
}

#[tokio::test]
async fn page_lifecycle_is_sequential_correlated_and_limited_once_per_page() {
    const FIRST_CURSOR: &str = "first-cursor-sentinel-a271";
    const SECOND_CURSOR: &str = "second-cursor-sentinel-f804";

    let executor = DeterministicExecutor::new();
    let delivery_gate = Gate::closed();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "http://pagination-runtime.example.test/v1/items?stable=public&cursor={FIRST_CURSOR}"
                ))
                .header(
                    http::HeaderName::from_static("x-page-cursor"),
                    http::HeaderValue::from_static(FIRST_CURSOR),
                ),
            ScriptedResponse::success(Bytes::from_static(
                b"first-item-sentinel-110b|second-cursor-sentinel-f804",
            ))
            .delivery_gate(delivery_gate.clone()),
        )
        .expect("first page response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "http://pagination-runtime.example.test/v1/items?stable=public&cursor={SECOND_CURSOR}"
                ))
                .header(
                    http::HeaderName::from_static("x-page-cursor"),
                    http::HeaderValue::from_static(SECOND_CURSOR),
                ),
            ScriptedResponse::success(Bytes::from_static(b"second-item-sentinel-7d92|-")),
        )
        .expect("second page response");
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        |runtime| runtime.debug(DebugLevel::Headers),
    )
    .with_debug_output(Arc::clone(&debug));

    let operation = tokio::spawn(collect(
        core,
        PageLimit::Prefix(5),
        CursorController::new(FIRST_CURSOR),
        Some(Duration::from_secs(30)),
    ));
    delivery_gate.wait_until_reached().await;

    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1,
        "the second page cannot start while the first response is active"
    );
    assert_eq!(
        limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .iter()
            .map(|(page, _)| *page)
            .collect::<Vec<_>>(),
        [0]
    );

    delivery_gate.open();
    let items = operation
        .await
        .expect("pagination task")
        .expect("pagination success");
    assert_eq!(
        items,
        ["first-item-sentinel-110b", "second-item-sentinel-7d92"]
    );
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        2
    );
    for capture in executor.application().captures().expect("captures") {
        assert!(capture.headers().iter().any(|header| {
            header.name() == "x-page-cursor" && header.value() == &CapturedHeaderValue::Redacted
        }));
        let rendered = format!("{capture:?}");
        assert!(!rendered.contains(FIRST_CURSOR));
        assert!(!rendered.contains(SECOND_CURSOR));
    }

    let before = hooks.before.lock().expect("before lock").clone();
    assert_eq!(before.len(), 2);
    assert_eq!(
        before
            .iter()
            .map(|record| record.page_index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(
        before
            .iter()
            .all(|record| record.attempt == INITIAL_ATTEMPT)
    );
    assert_ne!(before[0].operation_id, before[1].operation_id);
    assert_eq!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert!(before[0].parent_operation_id.is_some());
    for record in &before {
        assert!(!record.safe.contains(FIRST_CURSOR));
        assert!(!record.safe.contains(SECOND_CURSOR));
    }

    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 2);

    let aggregate = hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(aggregate.len(), 1);
    let aggregate = &aggregate[0];
    assert_eq!(before[0].parent_operation_id, Some(aggregate.operation_id));
    assert_eq!(aggregate.pages_completed, 2);
    assert_eq!(aggregate.items_collected, 2);
    assert!(!aggregate.recovered_authentication);
    assert_eq!(aggregate.final_status, Some(StatusCode::OK));
    assert_eq!(aggregate.outcome, CompletionOutcome::Success);
    assert_eq!(aggregate.attempts_started, 2);
    assert_eq!(aggregate.policy_retries, 0);
    assert!(aggregate.elapsed <= Duration::from_secs(30));

    assert_eq!(
        limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .iter()
            .map(|(page, origin)| {
                assert!(origin.is_none());
                *page
            })
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        limiter
            .observations
            .lock()
            .expect("observation lock")
            .as_slice(),
        [(0, StatusCode::OK), (1, StatusCode::OK)]
    );
    let rendered_debug = debug.lock().expect("debug output").join("\n");
    assert!(rendered_debug.contains("pagination-completion"));
    for forbidden in [
        FIRST_CURSOR,
        SECOND_CURSOR,
        "first-item-sentinel-110b",
        "second-item-sentinel-7d92",
    ] {
        assert!(!rendered_debug.contains(forbidden));
    }
}

#[tokio::test]
async fn one_page_retry_preserves_page_progress_and_aggregates_visible_attempts() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"intermediate-page-body"),
        ))
        .expect("first page retry status");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"first-item|second",
        )))
        .expect("first page success");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"second-item|-",
        )))
        .expect("second page success");
    let classifications = Arc::new(Mutex::new(Vec::new()));
    let classifications_for_policy = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .classify_fn(move |attempt| {
            classifications_for_policy
                .lock()
                .expect("pagination classifier records")
                .push((
                    attempt.page_index(),
                    attempt.attempt(),
                    attempt.host().map(str::to_owned),
                ));
            match attempt.failure() {
                RetryFailure::RejectedStatus {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    ..
                } => RetryDecision::Retry,
                _ => RetryDecision::Stop,
            }
        })
        .build()
        .expect("pagination retry policy");
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        |runtime| runtime.retry_policy(policy),
    );
    let controller = CursorController::new("first");
    let validate_calls = Arc::clone(&controller.validate_calls);
    let next_calls = Arc::clone(&controller.next_calls);
    let endpoint = PageEndpoint::get(Some(Duration::from_secs(30)), controller);
    let construction_calls = Arc::clone(&endpoint.construction_calls);
    let binding_calls = Arc::clone(&endpoint.binding_calls);

    let items = paginate_with(Request::new(core, endpoint), PageLimit::Prefix(5))
        .await
        .expect("one page retries without advancing the controller");
    assert_eq!(items, ["first-item", "second-item"]);
    assert_eq!(construction_calls.load(Ordering::SeqCst), 1);
    assert_eq!(validate_calls.load(Ordering::SeqCst), 2);
    assert_eq!(binding_calls.load(Ordering::SeqCst), 2);
    assert_eq!(next_calls.load(Ordering::SeqCst), 2);

    let before = hooks.before.lock().expect("before lock").clone();
    assert_eq!(
        before
            .iter()
            .map(|record| (record.page_index, record.attempt))
            .collect::<Vec<_>>(),
        [
            (0, INITIAL_ATTEMPT),
            (
                0,
                AttemptInfo {
                    index: 1,
                    policy_retry_index: 1,
                    authentication_recovery: false,
                },
            ),
            (1, INITIAL_ATTEMPT),
        ]
    );
    assert_eq!(before[0].operation_id, before[1].operation_id);
    assert_ne!(before[1].operation_id, before[2].operation_id);
    assert_eq!(
        classifications
            .lock()
            .expect("pagination classifier records")
            .as_slice(),
        [(
            0,
            INITIAL_ATTEMPT,
            Some("pagination-runtime.example.test".to_owned()),
        )]
    );

    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 2);
    let aggregate = hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 2);
    assert_eq!(aggregate[0].items_collected, 2);
    assert_eq!(aggregate[0].attempts_started, 3);
    assert_eq!(aggregate[0].policy_retries, 1);
    assert_eq!(
        limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .iter()
            .map(|(page, _)| *page)
            .collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert_eq!(
        limiter
            .observations
            .lock()
            .expect("observation lock")
            .as_slice(),
        [
            (0, StatusCode::SERVICE_UNAVAILABLE),
            (0, StatusCode::OK),
            (1, StatusCode::OK),
        ]
    );
}

#[tokio::test]
async fn non_clone_non_sync_controller_is_constructed_once_validated_and_bound_per_page() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"|second")))
        .expect("empty first page");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"item|-")))
        .expect("second page");
    let core = core(
        &executor,
        Arc::new(PaginationProbe::default()),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let controller = CursorController::new("first");
    let validate_calls = Arc::clone(&controller.validate_calls);
    let next_calls = Arc::clone(&controller.next_calls);
    let endpoint = PageEndpoint::get(Some(Duration::from_secs(30)), controller);
    let construction_calls = Arc::clone(&endpoint.construction_calls);
    let binding_calls = Arc::clone(&endpoint.binding_calls);
    let original = endpoint.clone();

    let items = paginate_with(Request::new(core, endpoint), PageLimit::Prefix(5))
        .await
        .expect("moved controller pagination");
    assert_eq!(items, ["item"]);
    assert_eq!(construction_calls.load(Ordering::SeqCst), 1);
    assert_eq!(validate_calls.load(Ordering::SeqCst), 2);
    assert_eq!(binding_calls.load(Ordering::SeqCst), 2);
    assert_eq!(next_calls.load(Ordering::SeqCst), 2);
    assert_eq!(original.cursor, "first");
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn protocol_recovery_does_not_repeat_pagination_preparation_or_page_lifecycle() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new().http2_prior_knowledge(),
        RuntimeConfigBuilder::new()
            .operation_observer(hooks.clone())
            .operation_observer(hooks.clone())
            .operation_observer(hooks.clone())
            .rate_limiter(configured_limiter),
    )
    .expect("HTTP/2 pagination core");
    let controller = CursorController::new("native-retry-cursor-sentinel-f120");
    let validate_calls = Arc::clone(&controller.validate_calls);
    let next_calls = Arc::clone(&controller.next_calls);
    let endpoint = PageEndpoint::get(Some(Duration::from_secs(30)), controller);
    let construction_calls = Arc::clone(&endpoint.construction_calls);

    let items = paginate_with(Request::new(core, endpoint), PageLimit::Prefix(1))
        .await
        .expect("private protocol-recovery pagination");
    assert!(items.is_empty());
    server.finish().await;

    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(construction_calls.load(Ordering::SeqCst), 1);
    assert_eq!(validate_calls.load(Ordering::SeqCst), 1);
    assert_eq!(next_calls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.before.lock().expect("before lock").len(), 1);
    assert_eq!(hooks.response.lock().expect("response lock").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 1);
    assert_eq!(hooks.aggregate.lock().expect("aggregate lock").len(), 1);
    assert_eq!(
        limiter.acquisitions.lock().expect("acquisition lock").len(),
        1
    );
    assert_eq!(
        limiter.observations.lock().expect("observation lock").len(),
        1
    );
}

#[tokio::test]
async fn cancellation_drops_request_local_state_and_prevents_later_pages() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"item|next")).delivery_gate(gate.clone()),
        )
        .expect("gated page");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"later|-")))
        .expect("unused later page");
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let core = core(&executor, Arc::clone(&hooks), limiter, |runtime| runtime);
    let task = tokio::spawn(collect(
        core,
        PageLimit::Prefix(5),
        CursorController::new("initial"),
        Some(Duration::from_secs(30)),
    ));
    gate.wait_until_reached().await;
    task.abort();
    assert!(task.await.expect_err("task cancellation").is_cancelled());
    gate.open();
    tokio::task::yield_now().await;

    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert!(hooks.completion.lock().expect("completion lock").is_empty());
    assert!(hooks.aggregate.lock().expect("aggregate lock").is_empty());
}

#[tokio::test]
async fn separate_collections_execute_concurrently_with_operation_local_identity_state() {
    let executor = DeterministicExecutor::new();
    let execution_gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(execution_gate.clone())
        .expect("execution gate");
    for item in ["first|-", "second|-"] {
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::copy_from_slice(
                item.as_bytes(),
            )))
            .expect("concurrent page");
    }
    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(LimiterProbe::default());
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::clone(&limiter),
        |runtime| runtime,
    );

    let first = tokio::spawn(collect(
        core.clone(),
        PageLimit::Prefix(1),
        CursorController::new("same-operation-local-cursor"),
        Some(Duration::from_secs(30)),
    ));
    let second = tokio::spawn(collect(
        core,
        PageLimit::Prefix(1),
        CursorController::new("same-operation-local-cursor"),
        Some(Duration::from_secs(30)),
    ));
    execution_gate.wait_until_arrivals(2).await;
    assert_eq!(
        limiter.acquisitions.lock().expect("acquisition lock").len(),
        2
    );
    execution_gate.open();
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.expect("first task").expect("first result").len(), 1);
    assert_eq!(
        second.expect("second task").expect("second result").len(),
        1
    );

    let before = hooks.before.lock().expect("before lock").clone();
    assert_eq!(before.len(), 2);
    assert_ne!(before[0].operation_id, before[1].operation_id);
    assert_ne!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert_eq!(hooks.aggregate.lock().expect("aggregate lock").len(), 2);
}
