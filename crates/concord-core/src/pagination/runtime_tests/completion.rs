use super::*;

#[tokio::test]
async fn later_page_errors_keep_ordinary_categories_and_aggregate_status() {
    let status_executor = DeterministicExecutor::new();
    status_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    status_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::TOO_MANY_REQUESTS,
            Bytes::from_static(b"status-body-sentinel"),
        ))
        .expect("status page");
    let status_hooks = Arc::new(PaginationProbe::default());
    let status_core = core(
        &status_executor,
        Arc::clone(&status_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            status_core,
            PageLimit::Prefix(5),
            CursorController::new("first"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.pagination_kind(), None);
    let aggregate = status_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(aggregate[0].items_collected, 1);
    assert_eq!(
        aggregate[0].final_status,
        Some(StatusCode::TOO_MANY_REQUESTS)
    );
    assert_eq!(aggregate[0].outcome, CompletionOutcome::HttpStatus);

    let decode_executor = DeterministicExecutor::new();
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"decoder-body-sentinel-without-structure",
        )))
        .expect("decode page");
    let decode_hooks = Arc::new(PaginationProbe::default());
    let decode_core = core(
        &decode_executor,
        Arc::clone(&decode_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            decode_core,
            PageLimit::Prefix(5),
            CursorController::new("first"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(error.pagination_kind(), None);
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains("decoder-body-sentinel"));
    let aggregate = decode_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(aggregate[0].final_status, Some(StatusCode::OK));
    assert_eq!(aggregate[0].outcome, CompletionOutcome::ResponseDecoding);

    let timeout_executor = DeterministicExecutor::new();
    timeout_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    timeout_executor
        .application()
        .queue_failure(TransportFailure::Timeout)
        .expect("timeout page");
    let timeout_hooks = Arc::new(PaginationProbe::default());
    let timeout_core = core(
        &timeout_executor,
        Arc::clone(&timeout_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            timeout_core,
            PageLimit::Prefix(5),
            CursorController::new("first"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.pagination_kind(), None);
    let aggregate = timeout_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Timeout);

    let rate_executor = DeterministicExecutor::new();
    let rate_hooks = Arc::new(PaginationProbe::default());
    let rejecting_limiter = Arc::new(RejectingLimiter::default());
    let configured_limiter: Arc<dyn RateLimiter> = rejecting_limiter.clone();
    let rate_core = ClientCore::build_with_development(
        "http://pagination-runtime.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .operation_observer(rate_hooks.clone())
            .operation_observer(rate_hooks.clone())
            .operation_observer(rate_hooks.clone())
            .rate_limiter(configured_limiter),
        &rate_executor,
    )
    .expect("rate-limit pagination core");
    let error = pagination_error(
        collect(
            rate_core,
            PageLimit::Prefix(1),
            CursorController::new("rate-limit-page"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.rate_limit_kind(), Some(RateLimitErrorKind::Acquire));
    assert_eq!(error.pagination_kind(), None);
    assert_eq!(rejecting_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        rate_executor.application().request_count().expect("count"),
        0
    );
    assert_eq!(
        rate_hooks.completion.lock().expect("completion lock").len(),
        1
    );
    let aggregate = rate_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 0);
    assert_eq!(aggregate[0].items_collected, 0);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::RateLimit);
}

#[tokio::test]
async fn pagination_emits_correlated_page_and_aggregate_terminal_observation() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|next")))
        .expect("first page");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"two|-")))
        .expect("second page");
    let hooks = Arc::new(PaginationProbe::default());
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let items = collect(
        core,
        PageLimit::Prefix(5),
        CursorController::new("first"),
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("pagination succeeds");
    assert_eq!(items, ["one", "two"]);
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(hooks.completion.lock().expect("completion lock").len(), 2);
    assert_eq!(hooks.aggregate.lock().expect("aggregate lock").len(), 1);
}

#[tokio::test]
async fn aggregate_completion_observes_one_fixed_result() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"one|-")))
        .expect("page");
    let hooks = Arc::new(PaginationProbe::default());
    let core = core(
        &executor,
        Arc::clone(&hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let items = collect(
        core,
        PageLimit::Prefix(1),
        CursorController::new("first"),
        Some(Duration::from_secs(1)),
    )
    .await
    .expect("fixed success");
    assert_eq!(items, ["one"]);
    let aggregate = hooks.aggregate.lock().expect("aggregate lock");
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Success);
}
