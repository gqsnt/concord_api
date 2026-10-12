use super::*;

fn assert_no_work(harness: &Harness, calls: &Calls) {
    assert_eq!(harness.requests(), 0);
    assert_eq!(harness.hooks.before_count(), 0);
    assert!(
        harness
            .limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    assert_eq!(Calls::get(&calls.inits), 0);
    assert_eq!(Calls::get(&calls.starts), 0);
    assert_eq!(Calls::get(&calls.clones), 0);
    assert_eq!(Calls::get(&calls.applies), 0);
}

fn assert_zero_summary(summary: &TraversalSummary) {
    assert_eq!(summary.stop_reason(), StopReason::PageBound);
    assert_eq!(summary.continuation(), ContinuationKnowledge::Unknown);
    assert_eq!(summary.operation_id(), None);
    assert_eq!(summary.requested_page_bound(), 0);
    assert_eq!(summary.effective_page_bound(), 0);
    assert_eq!(summary.pages_received(), 0);
    assert_eq!(summary.application_attempts(), 0);
    assert_eq!(summary.quota_admissions(), 0);
    assert_eq!(summary.acquired_response_bytes(), 0);
    assert_eq!(summary.retained_identity_bytes(), 0);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched
    );
}

/// V3-P04 / PAGE-02: zero bounds perform no shaping, body, controller,
/// credential, quota, transport or operation-identity work.
#[tokio::test]
async fn zero_bounds_perform_no_work_and_report_page_bound_unknown() {
    let harness = Harness::new();
    let endpoint = PageEndpoint::new("unused");
    let calls = endpoint.calls.clone();

    let zero_prefix = prefix(harness.core.clone(), endpoint.clone(), 0)
        .await
        .expect("zero prefix succeeds without work");
    assert!(zero_prefix.items().is_empty());
    assert_zero_summary(zero_prefix.summary());

    let mut pages: Pages<String> = pages_request(harness.request(endpoint), 0);
    assert_zero_summary(&pages.summary());
    assert!(pages.next_page().await.expect("zero pages").is_none());
    assert!(pages.next_page().await.expect("still none").is_none());
    let finished = pages.finish();
    assert_zero_summary(&finished);

    assert_no_work(&harness, &calls);
    assert!(harness.hooks.aggregate().is_empty());
}

/// PAGE-02: a nonzero traversal that never fetched finishes as an
/// application stop without identity or observation.
#[tokio::test]
async fn unstarted_nonzero_pages_finish_as_application_without_identity() {
    let harness = Harness::new();
    let endpoint = PageEndpoint::new("unused");
    let calls = endpoint.calls.clone();
    let pages: Pages<String> = pages_request(harness.request(endpoint), 3);
    let summary = pages.summary();
    assert_eq!(summary.stop_reason(), StopReason::NotStarted);
    assert_eq!(summary.requested_page_bound(), 3);
    assert_eq!(summary.effective_page_bound(), 3);
    assert_eq!(summary.operation_id(), None);

    let finished = pages.finish();
    assert_eq!(finished.stop_reason(), StopReason::Application);
    assert_eq!(finished.continuation(), ContinuationKnowledge::Unknown);
    assert_eq!(finished.operation_id(), None);
    assert_no_work(&harness, &calls);
    assert!(harness.hooks.aggregate().is_empty());

    // Dropping an unstarted traversal is equally inert.
    drop(pages_request::<PageEndpoint>(
        harness.request(PageEndpoint::new("unused")),
        2,
    ));
    assert!(harness.hooks.aggregate().is_empty());
}

/// CONTROL-02: a traversal timeout that is a valid control value but cannot
/// be represented as an absolute monotonic deadline from the first
/// `next_page` poll is rejected at that poll, before any effect: no
/// controller initialization or start, cursor application, shaping, limiter,
/// attempt or transport work, and `NotDispatched`.
#[tokio::test]
async fn unrepresentable_traversal_deadline_is_rejected_at_first_poll_before_effects() {
    let _clock = crate::operation::TestClockGuard::reset();
    let harness = Harness::new();
    let endpoint = PageEndpoint::new("unused");
    let calls = endpoint.calls.clone();
    let margin = Duration::from_secs(60 * 60);
    let request = harness
        .request(endpoint)
        .timeout(margin * 2)
        .expect("a representable control duration");
    let mut pages: Pages<String> = pages_request(request, 3);
    crate::operation::TestClockGuard::advance_to_representable_limit(margin);

    let error = pages
        .next_page()
        .await
        .expect_err("unrepresentable traversal deadline");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidValue)
    );
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(
        error.failure_stage(),
        crate::error::FailureStage::Preparation
    );
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(summary.pages_received(), 0);
    assert_eq!(summary.application_attempts(), 0);
    assert_eq!(summary.quota_admissions(), 0);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched
    );
    assert_no_work(&harness, &calls);
    // The failure is returned once; the traversal then stays terminal.
    assert!(pages.next_page().await.expect("terminal").is_none());
    assert_eq!(pages.finish().stop_reason(), StopReason::Failed);
    assert_no_work(&harness, &calls);
}

/// Requested bounds above the client ceiling are intersected and both numbers
/// are disclosed; a complete collection cut by the ceiling is an error.
#[tokio::test]
async fn requested_bound_is_intersected_with_the_profile_ceiling_and_disclosed() {
    let harness = Harness::configured(|runtime| runtime.pagination_max_pages(1));
    harness.queue("first|second");
    let prefix = prefix(harness.core.clone(), PageEndpoint::new("first"), 5)
        .await
        .expect("intersected prefix");
    assert_eq!(prefix.items(), ["first"]);
    let summary = prefix.summary();
    assert_eq!(summary.requested_page_bound(), 5);
    assert_eq!(summary.effective_page_bound(), 1);
    assert_eq!(summary.stop_reason(), StopReason::PageBound);
    assert_eq!(
        summary.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    assert_eq!(harness.requests(), 1, "no fetch beyond the effective bound");

    let complete = Harness::configured(|runtime| runtime.pagination_max_pages(1));
    complete.queue("first|second");
    let error = all(complete.core.clone(), PageEndpoint::new("first"), 5)
        .await
        .expect_err("ceiling-cut complete collection");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CompletionLimitExceeded)
    );
    let summary = summary_of(&error);
    assert_eq!(summary.requested_page_bound(), 5);
    assert_eq!(summary.effective_page_bound(), 1);
    assert_eq!(summary.stop_reason(), StopReason::PageBound);
    assert_eq!(complete.requests(), 1);
}

#[tokio::test]
async fn identity_ceiling_and_item_ceiling_cut_off_the_first_forbidden_work() {
    let identity = Harness::configured(|runtime| runtime.pagination_identity_limit(1));
    let error = prefix(
        identity.core.clone(),
        PageEndpoint::new("identity-first-excess-sentinel-6f11"),
        1,
    )
    .await
    .expect_err("identity ceiling");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::IdentityStorageCeilingExceeded)
    );
    assert!(!format!("{error:?}").contains("identity-first-excess-sentinel-6f11"));
    assert_eq!(identity.requests(), 0);
    assert_eq!(identity.hooks.before_count(), 0);
    assert!(
        identity
            .limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    assert_eq!(summary_of(&error).retained_identity_bytes(), 0);

    let items = Harness::configured(|runtime| runtime.pagination_max_items(1));
    items
        .executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"accepted-item-sentinel-62a3,first-excess-item-sentinel-029c|-",
        )))
        .expect("item page");
    let error = prefix(items.core.clone(), PageEndpoint::new("item-page"), 1)
        .await
        .expect_err("item ceiling");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CollectedItemCeilingExceeded)
    );
    assert_eq!(items.requests(), 1);
    let summary = summary_of(&error);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(summary.pages_delivered(), 0, "no truncated partial batch");
    assert_eq!(summary.items_delivered(), 0);
    let aggregate = items.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].items_collected, 0);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Pagination);
    assert_eq!(aggregate[0].final_status, Some(StatusCode::OK));
}

/// V3-P06: the identity budget accumulates across pages; a later
/// continuation that no longer fits withholds its page.
#[tokio::test]
async fn identity_budget_persists_across_pages() {
    let probe = Harness::new();
    probe.queue("a|c1");
    let mut pages: Pages<String> = pages_request(probe.request(PageEndpoint::new("c0")), 5);
    assert!(pages.next_page().await.expect("probe page").is_some());
    // Page 0 plus the validated page-1 continuation.
    let two_identities = pages.summary().retained_identity_bytes();
    assert!(two_identities > 0);
    assert_eq!(pages.finish().stop_reason(), StopReason::Application);

    let limited = Harness::configured(|runtime| runtime.pagination_identity_limit(two_identities));
    limited.queue("a|c1");
    limited.queue("b|c2");
    limited.queue("unused|-");
    let mut pages: Pages<String> = pages_request(limited.request(PageEndpoint::new("c0")), 5);
    assert!(pages.next_page().await.expect("fits").is_some());
    assert_eq!(pages.summary().retained_identity_bytes(), two_identities);
    let error = pages.next_page().await.expect_err("third identity");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::IdentityStorageCeilingExceeded)
    );
    let summary = summary_of(&error);
    assert_eq!(summary.pages_received(), 2);
    assert_eq!(summary.pages_delivered(), 1);
    assert_eq!(summary.retained_identity_bytes(), two_identities);
    assert_eq!(limited.requests(), 2);
}

#[tokio::test]
async fn aggregate_response_byte_budget_accepts_exact_total_and_rejects_first_excess() {
    const FIRST_PAGE: &str = "first-retained-item|second-state";
    const SECOND_PAGE: &str = "second-boundary-item|-";
    let exact_total = FIRST_PAGE.len() + SECOND_PAGE.len();

    let exact =
        Harness::configured(|runtime| runtime.pagination_total_response_body_limit(exact_total));
    exact.queue(FIRST_PAGE);
    exact.queue(SECOND_PAGE);
    let items = all(exact.core.clone(), PageEndpoint::new("first-state"), 2)
        .await
        .expect("exact aggregate response-byte boundary");
    assert_eq!(items, ["first-retained-item", "second-boundary-item"]);
    assert_eq!(exact.requests(), 2);

    let excess = Harness::configured(|runtime| {
        runtime.pagination_total_response_body_limit(exact_total - 1)
    });
    excess.queue(FIRST_PAGE);
    excess.queue(SECOND_PAGE);
    let endpoint = PageEndpoint::new("first-state");
    let calls = endpoint.calls.clone();
    let error = all(excess.core.clone(), endpoint, 2)
        .await
        .expect_err("first excess aggregate byte");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CollectedResponseByteCeilingExceeded)
    );
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains("first-retained-item"));
    assert!(!rendered.contains("second-boundary-item"));
    assert_eq!(excess.requests(), 2);
    assert_eq!(
        Calls::get(&calls.advances),
        1,
        "excess page is never interpreted"
    );
    let summary = summary_of(&error);
    assert_eq!(
        summary.acquired_response_bytes(),
        u64::try_from(exact_total).expect("small"),
        "the chunk that crossed the ceiling was actually read"
    );
    assert_eq!(summary.pages_received(), 1);
    let aggregate = excess.hooks.aggregate();
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(aggregate[0].items_collected, 1);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Pagination);

    // A caller response-byte cap narrows the traversal byte ceiling too.
    let capped = Harness::new();
    capped.queue(FIRST_PAGE);
    capped.queue(SECOND_PAGE);
    let error = collect_all_request(
        capped.request(PageEndpoint::new("first-state")),
        NonZeroU32::new(2).expect("nonzero"),
    )
    .with_options(
        &CallOptions::new()
            .cap_response_bytes(NonZeroUsize::new(FIRST_PAGE.len()).expect("positive cap")),
    )
    .await
    .expect_err("caller byte cap");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CollectedResponseByteCeilingExceeded)
    );
}

#[tokio::test]
async fn controller_start_failure_is_side_effect_free_and_redacted() {
    let harness = Harness::new();
    let mut endpoint = PageEndpoint::new("private-controller-state-sentinel-287e");
    endpoint.fail_start = true;
    let calls = endpoint.calls.clone();
    let error = prefix(harness.core.clone(), endpoint, 1)
        .await
        .expect_err("controller start fails");
    assert_eq!(error.category(), ErrorCategory::Pagination);
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    assert_eq!(
        error.stage(),
        crate::error::FailureStage::PaginationDecision
    );
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains("private-controller-state-sentinel-287e"));
    assert_eq!(Calls::get(&calls.starts), 1);
    assert_eq!(harness.requests(), 0);
    assert_eq!(harness.hooks.before_count(), 0);
    assert!(
        harness
            .hooks
            .completion
            .lock()
            .expect("completion lock")
            .is_empty()
    );
    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Pagination);
    assert_eq!(aggregate[0].stop_reason, StopReason::Failed);
    assert!(
        harness
            .limiter
            .acquisitions
            .lock()
            .expect("acquisition lock")
            .is_empty()
    );
    assert!(summary_of(&error).operation_id().is_some());
}

#[tokio::test]
async fn controller_selected_completion_and_empty_pages_follow_the_controller() {
    // A short page proves exhaustion even though the provider offered a cursor.
    let short = Harness::new();
    short.queue("|unused-next");
    let mut endpoint = PageEndpoint::new("empty");
    endpoint.short_page_size = Some(NonZeroUsize::MIN);
    endpoint.fail_advance = true;
    let items = all(short.core.clone(), endpoint, 1)
        .await
        .expect("controller-selected completion");
    assert!(items.is_empty());
    assert_eq!(short.requests(), 1);

    // An empty page continues when the controller offers continuation.
    let continuing = Harness::new();
    continuing.queue("|continued");
    continuing.queue("item|-");
    let items = all(continuing.core.clone(), PageEndpoint::new("empty-first"), 2)
        .await
        .expect("empty page continues");
    assert_eq!(items, ["item"]);

    // A controller error withholds the page but records it as received.
    let failing = Harness::new();
    failing.queue("item|unused");
    let mut endpoint = PageEndpoint::new("retained-error");
    endpoint.fail_advance = true;
    let error = all(failing.core.clone(), endpoint, 1)
        .await
        .expect_err("controller error");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Controller)
    );
    let summary = summary_of(&error);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(summary.pages_delivered(), 0);
    assert_eq!(summary.continuation(), ContinuationKnowledge::Unknown);
}

/// V3-P03: immediate repeats and longer cycles are rejected before the page
/// that proposed them is delivered, using full identity comparison.
#[tokio::test]
async fn immediate_two_state_and_long_cycles_are_withheld_before_delivery() {
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
        let harness = Harness::new();
        for page in *pages {
            harness
                .executor
                .application()
                .queue_response(ScriptedResponse::success(Bytes::copy_from_slice(
                    page.as_bytes(),
                )))
                .expect("cycle page");
        }
        let mut traversal: Pages<String> =
            pages_request(harness.request(PageEndpoint::new(initial)), 10);
        let mut delivered = Vec::new();
        let error = loop {
            match traversal.next_page().await {
                Ok(Some(page)) => delivered.extend(page.into_items()),
                Ok(None) => panic!("a cycle cannot end as exhaustion"),
                Err(error) => break error,
            }
        };
        assert_eq!(
            error.pagination_kind(),
            Some(PaginationErrorKind::RepeatedRequest)
        );
        assert_eq!(harness.requests(), pages.len());
        assert_eq!(harness.hooks.before_count(), pages.len());
        assert_eq!(
            delivered.len(),
            pages.len() - 1,
            "the cyclic page is withheld"
        );
        assert!(traversal.next_page().await.expect("after error").is_none());
        let summary = traversal.finish();
        assert_eq!(summary.stop_reason(), StopReason::Failed);
        assert_eq!(summary.pages_received() as usize, pages.len());
        assert_eq!(summary.pages_delivered() as usize, pages.len() - 1);
        let aggregate = harness.hooks.aggregate();
        assert_eq!(aggregate.len(), 1);
        assert_eq!(aggregate[0].pages_completed as usize, pages.len());
        assert_eq!(aggregate[0].items_collected, pages.len() - 1);
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

#[tokio::test]
async fn rejected_body_meter_keeps_status() {
    let harness = Harness::configured(|r| r.pagination_total_response_body_limit(8));
    harness.queue_status(StatusCode::SERVICE_UNAVAILABLE, "0123456789abcdef");
    let error = prefix(harness.core.clone(), PageEndpoint::new("initial"), 2)
        .await
        .expect_err("the status is rejected");
    assert_eq!(harness.requests(), 1);
    assert_eq!(
        error.last_response_status(),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(summary_of(&error).acquired_response_bytes(), 16);
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
}

#[tokio::test]
async fn rejected_body_meter_positive_controls() {
    let roomy = Harness::configured(|r| r.pagination_total_response_body_limit(64));
    roomy.queue_status(StatusCode::SERVICE_UNAVAILABLE, "0123456789abcdef");
    let error = prefix(roomy.core.clone(), PageEndpoint::new("initial"), 2)
        .await
        .expect_err("rejected status");
    assert_eq!(roomy.requests(), 1);
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(summary_of(&error).acquired_response_bytes(), 16);
    let direct = Harness::new();
    direct.queue_status(StatusCode::SERVICE_UNAVAILABLE, "0123456789abcdef");
    let error = direct
        .request(PageEndpoint::new("initial"))
        .with_options(&CallOptions::new().cap_response_bytes(NonZeroUsize::new(8).unwrap()))
        .await
        .err()
        .expect("direct status");
    assert_eq!(direct.requests(), 1);
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
}
