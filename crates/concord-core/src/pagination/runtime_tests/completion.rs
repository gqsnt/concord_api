use super::*;

async fn later_page_failure(configure: impl FnOnce(&Harness)) -> (ApiError, Vec<AggregateRecord>) {
    let harness = Harness::new();
    harness.queue("one|next");
    configure(&harness);
    let error = prefix(harness.core.clone(), PageEndpoint::new("first"), 5)
        .await
        .expect_err("later page fails");
    (error, harness.hooks.aggregate())
}

#[tokio::test]
async fn later_page_errors_keep_ordinary_categories_and_truthful_summaries() {
    let (error, aggregate) = later_page_failure(|harness| {
        harness.queue_status(StatusCode::TOO_MANY_REQUESTS, "status-body-sentinel");
    })
    .await;
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.pagination_kind(), None);
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(summary.pages_delivered(), 1);
    assert_eq!(summary.items_delivered(), 1);
    assert_eq!(summary.application_attempts(), 2);
    // The fetch failure does not erase the previously validated continuation.
    assert_eq!(
        summary.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    // The captured rejected-status body counts as acquired traversal bytes.
    assert_eq!(
        summary.acquired_response_bytes(),
        u64::try_from("one|next".len() + "status-body-sentinel".len()).expect("small")
    );
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(!format!("{error:?}").contains("status-body-sentinel"));
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(aggregate[0].items_collected, 1);
    assert_eq!(
        aggregate[0].final_status,
        Some(StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(aggregate[0].outcome, CompletionOutcome::HttpStatus);
    assert_eq!(aggregate[0].stop_reason, StopReason::Failed);

    let (error, aggregate) = later_page_failure(|harness| {
        harness.queue("decoder-body-sentinel-without-structure");
    })
    .await;
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(error.pagination_kind(), None);
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains("decoder-body-sentinel"));
    let summary = summary_of(&error);
    assert_eq!(
        summary.pages_received(),
        1,
        "an undecodable page is not received"
    );
    assert_eq!(
        summary.acquired_response_bytes(),
        u64::try_from("one|next".len() + "decoder-body-sentinel-without-structure".len())
            .expect("small")
    );
    assert_eq!(aggregate[0].final_status, Some(StatusCode::OK));
    assert_eq!(aggregate[0].outcome, CompletionOutcome::ResponseDecoding);

    let (error, aggregate) = later_page_failure(|harness| {
        harness
            .executor
            .application()
            .queue_failure(TransportFailure::Timeout)
            .expect("timeout page");
    })
    .await;
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.pagination_kind(), None);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Timeout);
    assert_eq!(summary_of(&error).stop_reason(), StopReason::Failed);
}

#[tokio::test]
async fn rate_admission_failure_on_the_first_page_stops_before_dispatch() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(PaginationProbe::default());
    let rejecting_limiter = Arc::new(RejectingLimiter::default());
    let configured_limiter: Arc<dyn RateLimiter> = rejecting_limiter.clone();
    let core = ClientCore::build_with_development(
        BASE,
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .operation_observer(hooks.clone())
            .operation_observer(hooks.clone())
            .operation_observer(hooks.clone())
            .rate_limiter(configured_limiter),
        &executor,
    )
    .expect("rate-limit pagination core");
    let error = prefix(core, PageEndpoint::new("rate-limit-page"), 1)
        .await
        .expect_err("rate admission fails");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.rate_limit_kind(), Some(RateLimitErrorKind::Acquire));
    assert_eq!(error.pagination_kind(), None);
    assert_eq!(rejecting_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let summary = summary_of(&error);
    assert_eq!(summary.quota_admissions(), 0);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched
    );
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 1);
    let aggregate = hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 0);
    assert_eq!(aggregate[0].items_collected, 0);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::RateLimit);
}

#[tokio::test]
async fn aggregate_completion_observes_one_fixed_result() {
    let harness = Harness::new();
    harness.queue("one|-");
    let items = all(
        harness.core.clone(),
        PageEndpoint::new("first").timeout(Duration::from_secs(1)),
        1,
    )
    .await
    .expect("fixed success");
    assert_eq!(items, ["one"]);
    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Success);
    assert_eq!(aggregate[0].stop_reason, StopReason::Exhausted);
}
