use super::*;

#[tokio::test]
async fn pagination_pages_use_the_declared_status_acceptance_policy() {
    let harness = Harness::new();
    harness
        .executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::NOT_FOUND,
            Bytes::from_static(b"accepted-page-item|-"),
        ))
        .expect("accepted pagination response");
    let endpoint = AcceptedPageEndpoint(PageEndpoint::new("initial"));

    let prefix = collect_prefix_request(Request::new(harness.core.clone(), endpoint), 1)
        .await
        .expect("accepted-status pagination page");
    assert_eq!(prefix.items(), ["accepted-page-item"]);
    assert_eq!(prefix.summary().stop_reason(), StopReason::Exhausted);
    assert_eq!(
        harness
            .limiter
            .observations
            .lock()
            .expect("observation lock")
            .as_slice(),
        [(0, StatusCode::NOT_FOUND)]
    );
    assert_eq!(
        harness.hooks.completion.lock().expect("completion").len(),
        1
    );
    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(aggregate[0].final_status, Some(StatusCode::NOT_FOUND));
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Success);
    assert_eq!(aggregate[0].stop_reason, StopReason::Exhausted);
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
                .native_url(format!("{BASE}items?stable=public&cursor={FIRST_CURSOR}"))
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
                .native_url(format!("{BASE}items?stable=public&cursor={SECOND_CURSOR}"))
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

    let operation = tokio::spawn(prefix(core, PageEndpoint::new(FIRST_CURSOR), 5));
    delivery_gate.wait_until_reached().await;

    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1,
        "the second page cannot start while the first response is active"
    );
    assert_eq!(limiter.acquisition_pages(), [0]);

    delivery_gate.open();
    let prefix = operation
        .await
        .expect("pagination task")
        .expect("pagination success");
    assert_eq!(
        prefix.items(),
        ["first-item-sentinel-110b", "second-item-sentinel-7d92"]
    );
    let summary = prefix.summary();
    assert_eq!(summary.stop_reason(), StopReason::Exhausted);
    assert_eq!(summary.continuation(), ContinuationKnowledge::Exhausted);
    assert_eq!(summary.pages_received(), 2);
    assert_eq!(summary.pages_delivered(), 2);
    assert_eq!(summary.items_delivered(), 2);
    assert_eq!(summary.application_attempts(), 2);
    assert_eq!(summary.automatic_replays(), 0);
    assert_eq!(summary.quota_admissions(), 2);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
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

    let aggregate = hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    let aggregate = &aggregate[0];
    assert_eq!(before[0].parent_operation_id, Some(aggregate.operation_id));
    assert_eq!(summary.operation_id(), Some(aggregate.operation_id));
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
    assert!(rendered_debug.contains("stop=Exhausted"));
    for forbidden in [
        FIRST_CURSOR,
        SECOND_CURSOR,
        "first-item-sentinel-110b",
        "second-item-sentinel-7d92",
    ] {
        assert!(!rendered_debug.contains(forbidden));
    }
    let rendered_summary = format!("{summary:?}");
    assert!(!rendered_summary.contains(FIRST_CURSOR));
    assert!(!rendered_summary.contains(SECOND_CURSOR));
}

/// RETRY-01: a pagination controller `advance()` error is terminal even under
/// a policy that recommends a replay for every failure: the classifier is
/// never consulted, the page is requested once and its batch is withheld.
#[tokio::test]
async fn retry_01_controller_advance_error_is_terminal_under_an_always_retry_policy() {
    let classifications = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(16)
        .without_budget()
        .classify_fn(move |_attempt| {
            seen.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        })
        .build()
        .expect("always-retry policy");
    let harness = Harness::configured(|runtime| runtime.retry_policy(policy));
    harness.queue("withheld-item|unused-next");
    harness.queue("never-requested|-");
    let mut endpoint = PageEndpoint::new("advance-error");
    endpoint.fail_advance = true;
    let calls = endpoint.calls.clone();

    let error = all(harness.core.clone(), endpoint, 3)
        .await
        .expect_err("controller advance error");
    assert_eq!(error.category(), ErrorCategory::Pagination);
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    assert_eq!(
        error.failure_stage(),
        crate::error::FailureStage::PaginationDecision
    );
    assert_eq!(classifications.load(Ordering::SeqCst), 0);
    assert_eq!(harness.requests(), 1);
    assert_eq!(Calls::get(&calls.advances), 1);
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(summary.application_attempts(), 1);
    assert_eq!(summary.automatic_replays(), 0);
    assert_eq!(summary.quota_admissions(), 1);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(summary.pages_delivered(), 0);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
}

#[tokio::test]
async fn one_page_retry_preserves_page_progress_and_aggregates_visible_attempts() {
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
    let harness = Harness::configured(|runtime| runtime.retry_policy(policy));
    harness.queue_status(StatusCode::SERVICE_UNAVAILABLE, "intermediate-page-body");
    harness.queue("first-item|second");
    harness.queue("second-item|-");
    let endpoint = PageEndpoint::new("first");
    let calls = endpoint.calls.clone();

    let prefix = prefix(harness.core.clone(), endpoint, 5)
        .await
        .expect("one page retries without advancing the controller");
    assert_eq!(prefix.items(), ["first-item", "second-item"]);
    assert_eq!(Calls::get(&calls.starts), 1);
    assert_eq!(Calls::get(&calls.advances), 2);
    // Initial-cursor check plus one validated continuation.
    assert_eq!(Calls::get(&calls.applies), 2);
    let summary = prefix.summary();
    assert_eq!(summary.application_attempts(), 3);
    assert_eq!(summary.automatic_replays(), 1);
    assert_eq!(summary.quota_admissions(), 3);
    assert_eq!(summary.pages_received(), 2);
    // The retried response was dropped without reading its body.
    assert_eq!(
        summary.acquired_response_bytes(),
        u64::try_from("first-item|second".len() + "second-item|-".len()).expect("small")
    );

    let before = harness.hooks.before.lock().expect("before lock").clone();
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

    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 2);
    assert_eq!(aggregate[0].items_collected, 2);
    assert_eq!(aggregate[0].attempts_started, 3);
    assert_eq!(aggregate[0].policy_retries, 1);
    assert_eq!(harness.limiter.acquisition_pages(), [0, 0, 1]);
    assert_eq!(
        harness
            .limiter
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
async fn non_clone_non_sync_controller_is_started_once_and_cursor_bound_per_page() {
    let harness = Harness::new();
    harness.queue("|second");
    harness.queue("item|-");
    let endpoint = PageEndpoint::new("first");
    let calls = endpoint.calls.clone();
    let original = endpoint.clone();

    let items = all(harness.core.clone(), endpoint, 5)
        .await
        .expect("moved controller pagination");
    assert_eq!(items, ["item"]);
    assert_eq!(Calls::get(&calls.inits), 1);
    assert_eq!(Calls::get(&calls.starts), 1);
    assert_eq!(Calls::get(&calls.advances), 2);
    assert_eq!(Calls::get(&calls.applies), 2);
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
    let endpoint = PageEndpoint::new("native-retry-cursor-sentinel-f120");
    let calls = endpoint.calls.clone();

    let prefix = prefix(core, endpoint, 1)
        .await
        .expect("private protocol-recovery pagination");
    assert!(prefix.items().is_empty());
    server.finish().await;

    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(Calls::get(&calls.starts), 1);
    assert_eq!(Calls::get(&calls.advances), 1);
    assert_eq!(prefix.summary().application_attempts(), 1);
    assert_eq!(prefix.summary().quota_admissions(), 1);
    assert_eq!(hooks.before_count(), 1);
    assert_eq!(hooks.response.lock().expect("response lock").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 1);
    assert_eq!(hooks.aggregate().len(), 1);
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
async fn aborting_a_started_collector_records_cancellation_and_prevents_later_pages() {
    let harness = Harness::new();
    let gate = Gate::closed();
    harness
        .executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"item|next")).delivery_gate(gate.clone()),
        )
        .expect("gated page");
    harness.queue("later|-");
    let task = tokio::spawn(prefix(
        harness.core.clone(),
        PageEndpoint::new("initial"),
        5,
    ));
    gate.wait_until_reached().await;
    task.abort();
    assert!(task.await.expect_err("task cancellation").is_cancelled());
    gate.open();
    tokio::task::yield_now().await;

    assert_eq!(harness.requests(), 1);
    // The in-flight page operation never fixed a result of its own; its
    // synchronous drop guard records exactly one `Cancelled` page completion
    // (CONTROL-03) before the traversal records the aggregate cancellation.
    assert_eq!(
        harness
            .hooks
            .completion
            .lock()
            .expect("completion lock")
            .len(),
        1
    );
    // The traversal drop guard records the best bounded known completion.
    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].stop_reason, StopReason::Cancelled);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Cancelled);
    assert_eq!(aggregate[0].pages_completed, 0);
    assert_eq!(aggregate[0].attempts_started, 1);
}

#[tokio::test]
async fn separate_collections_execute_concurrently_with_operation_local_identity_state() {
    let harness = Harness::new();
    let execution_gate = Gate::closed();
    harness
        .executor
        .application()
        .pause_before_execution(execution_gate.clone())
        .expect("execution gate");
    harness.queue("first|-");
    harness.queue("second|-");

    let first = tokio::spawn(prefix(
        harness.core.clone(),
        PageEndpoint::new("same-operation-local-cursor"),
        1,
    ));
    let second = tokio::spawn(prefix(
        harness.core.clone(),
        PageEndpoint::new("same-operation-local-cursor"),
        1,
    ));
    execution_gate.wait_until_arrivals(2).await;
    assert_eq!(
        harness
            .limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .len(),
        2
    );
    execution_gate.open();
    let (first, second) = tokio::join!(first, second);
    assert_eq!(
        first
            .expect("first task")
            .expect("first result")
            .items()
            .len(),
        1
    );
    assert_eq!(
        second
            .expect("second task")
            .expect("second result")
            .items()
            .len(),
        1
    );

    let before = harness.hooks.before.lock().expect("before lock").clone();
    assert_eq!(before.len(), 2);
    assert_ne!(before[0].operation_id, before[1].operation_id);
    assert_ne!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert_eq!(harness.hooks.aggregate().len(), 2);
}
