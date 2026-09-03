use super::*;

#[tokio::test]
async fn zero_page_and_configured_ceilings_cut_off_the_first_forbidden_work() {
    let zero_executor = DeterministicExecutor::new();
    let zero_hooks = Arc::new(PaginationProbe::default());
    let zero_limiter = Arc::new(LimiterProbe::default());
    let zero_core = core(
        &zero_executor,
        Arc::clone(&zero_hooks),
        Arc::clone(&zero_limiter),
        |runtime| runtime,
    );
    let zero_endpoint = PageEndpoint::get(
        Some(Duration::from_secs(30)),
        CursorController::new("unused"),
    );
    let zero_construction_calls = Arc::clone(&zero_endpoint.construction_calls);
    let zero = paginate_with(Request::new(zero_core, zero_endpoint), PageLimit::Prefix(0))
        .await
        .expect("zero-page success");
    assert!(zero.is_empty());
    assert_eq!(zero_construction_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        zero_executor.application().request_count().expect("count"),
        0
    );
    assert!(zero_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        zero_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    let zero_aggregate = zero_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert!(zero_aggregate.is_empty());

    let page_executor = DeterministicExecutor::new();
    let page_hooks = Arc::new(PaginationProbe::default());
    let page_limiter = Arc::new(LimiterProbe::default());
    let page_core = core(
        &page_executor,
        Arc::clone(&page_hooks),
        Arc::clone(&page_limiter),
        |runtime| runtime.pagination_max_pages(NonZeroU32::MIN),
    );
    let page_endpoint = PageEndpoint::get(
        Some(Duration::from_secs(30)),
        CursorController::new("unused"),
    );
    let page_construction_calls = Arc::clone(&page_endpoint.construction_calls);
    let error = pagination_error(
        paginate_with(Request::new(page_core, page_endpoint), PageLimit::Prefix(2)).await,
    );
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::PageCeilingExceeded)
    );
    assert_eq!(page_construction_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        page_executor.application().request_count().expect("count"),
        0
    );
    assert!(page_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        page_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    let page_aggregate = page_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(page_aggregate.len(), 1);
    assert_eq!(page_aggregate[0].pages_completed, 0);
    assert_eq!(page_aggregate[0].items_collected, 0);
    assert_eq!(page_aggregate[0].final_status, None);
    assert_eq!(page_aggregate[0].outcome, CompletionOutcome::Pagination);

    let maximum_executor = DeterministicExecutor::new();
    maximum_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"first|second",
        )))
        .expect("first maximum page");
    maximum_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"")))
        .expect("natural maximum page");
    let maximum_hooks = Arc::new(PaginationProbe::default());
    let maximum_limiter = Arc::new(LimiterProbe::default());
    let maximum_core = core(
        &maximum_executor,
        Arc::clone(&maximum_hooks),
        maximum_limiter,
        |runtime| runtime.pagination_max_pages(NonZeroU32::new(2).expect("positive page ceiling")),
    );
    let maximum = collect(
        maximum_core,
        PageLimit::CompleteWithin(NonZeroU32::new(2).expect("positive completion limit")),
        CursorController::new("first"),
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("configured maximum naturally terminates");
    assert_eq!(maximum, ["first"]);
    assert_eq!(
        maximum_executor
            .application()
            .request_count()
            .expect("count"),
        2
    );
    let maximum_aggregate = maximum_hooks
        .aggregate
        .lock()
        .expect("aggregate lock")
        .clone();
    assert_eq!(maximum_aggregate.len(), 1);
    assert_eq!(maximum_aggregate[0].pages_completed, 2);
    assert_eq!(maximum_aggregate[0].items_collected, 1);
    assert_eq!(maximum_aggregate[0].outcome, CompletionOutcome::Success);

    let identity_executor = DeterministicExecutor::new();
    let identity_hooks = Arc::new(PaginationProbe::default());
    let identity_limiter = Arc::new(LimiterProbe::default());
    let identity_core = core(
        &identity_executor,
        Arc::clone(&identity_hooks),
        Arc::clone(&identity_limiter),
        |runtime| runtime.pagination_identity_limit(NonZeroUsize::MIN),
    );
    let identity_error = pagination_error(
        collect(
            identity_core,
            PageLimit::Prefix(1),
            CursorController::new("identity-first-excess-sentinel-6f11"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(
        identity_error.pagination_kind(),
        Some(PaginationErrorKind::IdentityStorageCeilingExceeded)
    );
    assert_eq!(
        identity_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );
    assert!(
        identity_hooks
            .before
            .lock()
            .expect("before lock")
            .is_empty()
    );
    assert!(
        identity_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );

    let item_executor = DeterministicExecutor::new();
    item_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"accepted-item-sentinel-62a3,first-excess-item-sentinel-029c|-",
        )))
        .expect("item page");
    let item_hooks = Arc::new(PaginationProbe::default());
    let item_limiter = Arc::new(LimiterProbe::default());
    let item_core = core(
        &item_executor,
        Arc::clone(&item_hooks),
        item_limiter,
        |runtime| runtime.pagination_max_items(NonZeroUsize::MIN),
    );
    let item_error = pagination_error(
        collect(
            item_core,
            PageLimit::Prefix(1),
            CursorController::new("item-page"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(
        item_error.pagination_kind(),
        Some(PaginationErrorKind::CollectedItemCeilingExceeded)
    );
    assert_eq!(
        item_executor.application().request_count().expect("count"),
        1
    );
    let item_aggregate = item_hooks.aggregate.lock().expect("aggregate lock").clone();
    assert_eq!(item_aggregate.len(), 1);
    assert_eq!(item_aggregate[0].items_collected, 1);
    assert_eq!(item_aggregate[0].outcome, CompletionOutcome::Pagination);
    assert_eq!(item_aggregate[0].final_status, Some(StatusCode::OK));
}

#[tokio::test]
async fn aggregate_response_body_budget_accepts_exact_total_and_drops_first_excess_page() {
    const FIRST_PAGE: &[u8] = b"first-retained-item|second-state";
    const SECOND_PAGE: &[u8] = b"second-boundary-item|-";
    let exact_total = FIRST_PAGE
        .len()
        .checked_add(SECOND_PAGE.len())
        .expect("small test response lengths");

    let exact_executor = DeterministicExecutor::new();
    for page in [FIRST_PAGE, SECOND_PAGE] {
        exact_executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(page)))
            .expect("exact aggregate response page");
    }
    let exact_hooks = Arc::new(PaginationProbe::default());
    let exact_core = core(
        &exact_executor,
        Arc::clone(&exact_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| {
            runtime.pagination_total_response_body_limit(
                NonZeroUsize::new(exact_total).expect("positive exact aggregate limit"),
            )
        },
    );
    let exact = collect(
        exact_core,
        PageLimit::CompleteWithin(NonZeroU32::new(2).expect("two-page completion limit")),
        CursorController::new("first-state"),
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("exact aggregate response-byte boundary");
    assert_eq!(exact, ["first-retained-item", "second-boundary-item"]);
    assert_eq!(
        exact_executor.application().request_count().expect("count"),
        2
    );
    {
        let exact_aggregate = exact_hooks.aggregate.lock().expect("aggregate lock");
        assert_eq!(exact_aggregate[0].pages_completed, 2);
        assert_eq!(exact_aggregate[0].items_collected, 2);
        assert_eq!(exact_aggregate[0].outcome, CompletionOutcome::Success);
    }

    let excess_executor = DeterministicExecutor::new();
    for page in [FIRST_PAGE, SECOND_PAGE] {
        excess_executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(page)))
            .expect("first-excess aggregate response page");
    }
    let excess_hooks = Arc::new(PaginationProbe::default());
    let excess_core = core(
        &excess_executor,
        Arc::clone(&excess_hooks),
        Arc::new(LimiterProbe::default()),
        |runtime| {
            runtime.pagination_total_response_body_limit(
                NonZeroUsize::new(exact_total - 1).expect("positive first-excess limit"),
            )
        },
    );
    let controller = CursorController::new("first-state");
    let next_calls = Arc::clone(&controller.next_calls);
    let error = pagination_error(
        collect(
            excess_core,
            PageLimit::CompleteWithin(NonZeroU32::new(2).expect("two-page completion limit")),
            controller,
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CollectedResponseByteCeilingExceeded)
    );
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains("first-retained-item"));
    assert!(!rendered.contains("second-boundary-item"));
    assert_eq!(
        excess_executor
            .application()
            .request_count()
            .expect("count"),
        2
    );
    assert_eq!(next_calls.load(Ordering::SeqCst), 1);
    assert!(
        excess_hooks
            .completion
            .lock()
            .expect("completion lock")
            .is_empty()
    );
    let excess_aggregate = excess_hooks.aggregate.lock().expect("aggregate lock");
    assert_eq!(excess_aggregate[0].pages_completed, 1);
    assert_eq!(excess_aggregate[0].items_collected, 1);
    assert_eq!(excess_aggregate[0].final_status, Some(StatusCode::OK));
    assert_eq!(excess_aggregate[0].outcome, CompletionOutcome::Pagination);
}

#[tokio::test]
async fn controller_construction_and_validation_failures_are_page_side_effect_free() {
    let factory_executor = DeterministicExecutor::new();
    let factory_hooks = Arc::new(PaginationProbe::default());
    let factory_limiter = Arc::new(LimiterProbe::default());
    let factory_core = core(
        &factory_executor,
        Arc::clone(&factory_hooks),
        Arc::clone(&factory_limiter),
        |runtime| runtime,
    );
    let factory_error = pagination_error(
        paginate_with(
            Request::new(
                factory_core,
                PageEndpoint::failing_construction(Some(Duration::from_secs(30))),
            ),
            PageLimit::Prefix(1),
        )
        .await,
    );
    assert_eq!(factory_error.category(), ErrorCategory::Pagination);
    assert_eq!(
        factory_error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    assert_eq!(
        factory_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );
    assert!(factory_hooks.before.lock().expect("before lock").is_empty());
    assert!(
        factory_hooks
            .completion
            .lock()
            .expect("completion lock")
            .is_empty()
    );
    assert_eq!(
        factory_hooks.aggregate.lock().expect("aggregate lock")[0].outcome,
        CompletionOutcome::Pagination
    );
    assert!(
        factory_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );

    let validate_executor = DeterministicExecutor::new();
    let validate_hooks = Arc::new(PaginationProbe::default());
    let validate_limiter = Arc::new(LimiterProbe::default());
    let validate_core = core(
        &validate_executor,
        Arc::clone(&validate_hooks),
        Arc::clone(&validate_limiter),
        |runtime| runtime,
    );
    let controller = CursorController::failing_validate("private-controller-state-sentinel-287e");
    let validate_calls = Arc::clone(&controller.validate_calls);
    let validate_error = pagination_error(
        collect(
            validate_core,
            PageLimit::Prefix(1),
            controller,
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(validate_calls.load(Ordering::SeqCst), 1);
    assert_eq!(validate_error.category(), ErrorCategory::Pagination);
    assert_eq!(
        validate_error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    let rendered = format!("{validate_error}\n{validate_error:?}");
    assert!(!rendered.contains("private-controller-state-sentinel-287e"));
    assert_eq!(
        validate_executor
            .application()
            .request_count()
            .expect("count"),
        0
    );
    assert!(
        validate_hooks
            .before
            .lock()
            .expect("before lock")
            .is_empty()
    );
    assert!(
        validate_hooks
            .completion
            .lock()
            .expect("completion lock")
            .is_empty()
    );
    assert_eq!(
        validate_hooks.aggregate.lock().expect("aggregate lock")[0].outcome,
        CompletionOutcome::Pagination
    );
    assert!(
        validate_limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
}

#[tokio::test]
async fn final_soft_and_controller_selected_count_pages_apply_exact_next_precedence() {
    let soft_executor = DeterministicExecutor::new();
    soft_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"item|unused-next",
        )))
        .expect("soft page");
    let soft_hooks = Arc::new(PaginationProbe::default());
    let soft_core = core(
        &soft_executor,
        soft_hooks,
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let controller = CursorController::new("soft");
    let next_calls = Arc::clone(&controller.next_calls);
    let items = collect(
        soft_core,
        PageLimit::Prefix(1),
        controller,
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("final soft page");
    assert_eq!(items, ["item"]);
    assert_eq!(next_calls.load(Ordering::SeqCst), 0);

    let empty_executor = DeterministicExecutor::new();
    empty_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"|unused-next",
        )))
        .expect("empty page");
    let empty_core = core(
        &empty_executor,
        Arc::new(PaginationProbe::default()),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let controller =
        CursorController::failing_next("empty").with_short_page_size(NonZeroUsize::MIN);
    let next_calls = Arc::clone(&controller.next_calls);
    let items = collect(
        empty_core,
        PageLimit::CompleteWithin(NonZeroU32::MIN),
        controller,
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("controller-selected count stop overrides retained next error");
    assert!(items.is_empty());
    assert_eq!(next_calls.load(Ordering::SeqCst), 1);

    let continuing_executor = DeterministicExecutor::new();
    continuing_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"|continued")))
        .expect("empty continuing page");
    continuing_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"item|-")))
        .expect("page after empty page");
    let continuing_core = core(
        &continuing_executor,
        Arc::new(PaginationProbe::default()),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let items = collect(
        continuing_core,
        PageLimit::CompleteWithin(NonZeroU32::new(2).expect("two-page cap")),
        CursorController::new("empty-first"),
        Some(Duration::from_secs(30)),
    )
    .await
    .expect("empty page continues when the controller selects no count termination");
    assert_eq!(items, ["item"]);

    let retained_error_executor = DeterministicExecutor::new();
    retained_error_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"item|unused",
        )))
        .expect("non-count-terminated page");
    let retained_error_core = core(
        &retained_error_executor,
        Arc::new(PaginationProbe::default()),
        Arc::new(LimiterProbe::default()),
        |runtime| runtime,
    );
    let error = pagination_error(
        collect(
            retained_error_core,
            PageLimit::CompleteWithin(NonZeroU32::MIN),
            CursorController::failing_next("retained-error"),
            Some(Duration::from_secs(30)),
        )
        .await,
    );
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
}

#[tokio::test]
async fn immediate_two_state_and_long_cycles_stop_before_protected_page_work() {
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "immediate-cycle-sentinel-8251",
            &["one|immediate-cycle-sentinel-8251"],
            &["immediate-cycle-sentinel-8251"],
        ),
        (
            "two-a-cycle-sentinel-e3d1",
            &[
                "one|two-b-cycle-sentinel-a044",
                "two|two-a-cycle-sentinel-e3d1",
            ],
            &["two-a-cycle-sentinel-e3d1", "two-b-cycle-sentinel-a044"],
        ),
        (
            "long-a-cycle-sentinel-a31b",
            &[
                "one|long-b-cycle-sentinel-bf11",
                "two|long-c-cycle-sentinel-48ce",
                "three|long-a-cycle-sentinel-a31b",
            ],
            &[
                "long-a-cycle-sentinel-a31b",
                "long-b-cycle-sentinel-bf11",
                "long-c-cycle-sentinel-48ce",
            ],
        ),
    ];

    for (initial, pages, forbidden) in cases {
        let executor = DeterministicExecutor::new();
        for page in *pages {
            executor
                .application()
                .queue_response(ScriptedResponse::success(Bytes::copy_from_slice(
                    page.as_bytes(),
                )))
                .expect("cycle page");
        }
        let hooks = Arc::new(PaginationProbe::default());
        let limiter = Arc::new(LimiterProbe::default());
        let core = core(
            &executor,
            Arc::clone(&hooks),
            Arc::clone(&limiter),
            |runtime| runtime,
        );
        let error = pagination_error(
            collect(
                core,
                PageLimit::Prefix(10),
                CursorController::new(*initial),
                Some(Duration::from_secs(30)),
            )
            .await,
        );
        assert_eq!(
            error.pagination_kind(),
            Some(PaginationErrorKind::RepeatedRequest)
        );
        assert_eq!(
            executor.application().request_count().expect("count"),
            pages.len()
        );
        assert_eq!(hooks.before.lock().expect("before lock").len(), pages.len());
        assert_eq!(
            limiter.acquisitions.lock().expect("acquisition lock").len(),
            pages.len()
        );
        let aggregate = hooks.aggregate.lock().expect("aggregate lock").clone();
        assert_eq!(aggregate.len(), 1);
        assert_eq!(aggregate[0].pages_completed as usize, pages.len());
        assert_eq!(aggregate[0].items_collected, pages.len());
        assert_eq!(aggregate[0].outcome, CompletionOutcome::Pagination);

        let mut rendered = vec![error.to_string(), format!("{error:?}")];
        let mut source = error.source();
        while let Some(error) = source {
            rendered.push(error.to_string());
            rendered.push(format!("{error:?}"));
            source = error.source();
        }
        let rendered = rendered.join("\n");
        for sentinel in *forbidden {
            assert!(!rendered.contains(sentinel));
        }
    }
}
