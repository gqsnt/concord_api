//! Terminal laws: initial cursor identity, continuation validation, bounds,
//! complete/prefix/lazy results, the aggregate deadline, persistent budgets,
//! and cancellation.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll};

use crate::controls::{ControlError, RequestControls};

use super::*;

fn assert_send<T: Send>(_: &T) {}

fn poll_once<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
    let waker = std::task::Waker::noop();
    let mut context = Context::from_waker(waker);
    future.poll(&mut context)
}

#[test]
fn terminals_and_their_futures_are_send_and_inert() {
    let harness = Harness::new();
    let endpoint = PageEndpoint::new("inert");
    let calls = endpoint.calls.clone();
    let mut pages: Pages<String> = pages_request(harness.request(endpoint.clone()), 2);
    assert_send(&pages);
    {
        let next = pages.next_page();
        assert_send(&next);
    }
    let prefix: CollectPrefix<String> =
        collect_prefix_request(harness.request(endpoint.clone()), 2);
    assert_send(&prefix);
    let complete: CollectAll<String> =
        collect_all_request(harness.request(endpoint), NonZeroU32::MIN);
    assert_send(&complete);
    let prefix_future = prefix.into_future();
    assert_send(&prefix_future);
    let complete_future = complete.into_future();
    assert_send(&complete_future);
    drop((prefix_future, complete_future, pages));
    assert_eq!(harness.requests(), 0);
    assert_eq!(Calls::get(&calls.starts), 0);
    assert_eq!(Calls::get(&calls.inits), 0);
    assert!(harness.hooks.aggregate().is_empty());
}

/// V3-P03: the initial cursor must reproduce the current request identity;
/// the mismatch is detected before any credential, quota or transport work.
#[tokio::test]
async fn initial_cursor_mismatch_is_rejected_before_send() {
    let harness = Harness::new();
    harness.queue("unused|-");
    let mut endpoint = PageEndpoint::new("slot-cursor-sentinel-5d10");
    endpoint.init_cursor = Some("init-cursor-sentinel-77e2".to_owned());
    let calls = endpoint.calls.clone();
    let error = prefix(harness.core.clone(), endpoint, 3)
        .await
        .expect_err("initial cursor mismatch");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::InvalidInitialCursor)
    );
    assert_eq!(harness.requests(), 0);
    assert_eq!(harness.hooks.before_count(), 0);
    assert!(
        harness
            .limiter
            .acquisitions
            .lock()
            .expect("acquisitions")
            .is_empty()
    );
    assert_eq!(Calls::get(&calls.starts), 1);
    assert_eq!(Calls::get(&calls.advances), 0);
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert!(summary.operation_id().is_some());
    assert_eq!(summary.application_attempts(), 0);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::NotDispatched
    );
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("slot-cursor-sentinel-5d10"));
    assert!(!rendered.contains("init-cursor-sentinel-77e2"));
}

/// V3-P03: an invalid proposed next cursor withholds the current page, which
/// is still recorded as received; the error is returned once.
#[tokio::test]
async fn invalid_next_cursor_withholds_the_current_page() {
    let harness = Harness::new();
    harness.queue("first|second");
    harness.queue("withheld-item-sentinel|bad");
    let mut endpoint = PageEndpoint::new("first");
    // A header value cannot contain a line break: the continuation fails
    // public shaping after `second` proposes it.
    endpoint.rewrite = Some(|next| {
        if next == "bad" {
            "bad\ninjected".to_owned()
        } else {
            next.to_owned()
        }
    });
    let mut pages: Pages<String> = pages_request(harness.request(endpoint), 10);
    let first = pages
        .next_page()
        .await
        .expect("first page")
        .expect("delivered");
    assert_eq!(first.into_items(), ["first"]);
    let error = pages.next_page().await.expect_err("invalid continuation");
    assert_ne!(error.category(), ErrorCategory::Pagination);
    assert!(!format!("{error:?}").contains("withheld-item-sentinel"));
    let summary = summary_of(&error);
    assert_eq!(summary.pages_received(), 2);
    assert_eq!(summary.pages_delivered(), 1);
    assert_eq!(summary.items_delivered(), 1);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(summary.continuation(), ContinuationKnowledge::Unknown);
    assert!(pages.next_page().await.expect("returned once").is_none());
    assert!(pages.next_page().await.expect("still none").is_none());
    assert_eq!(pages.finish().stop_reason(), StopReason::Failed);
    assert_eq!(harness.requests(), 2);
}

/// V3-P04: a `Done` on the final permitted page is exhaustion without an
/// extra fetch; an offered continuation at the bound is `PageBound` with a
/// validated next request, also without an extra fetch.
#[tokio::test]
async fn exact_bound_exhaustion_is_distinct_from_offered_continuation() {
    let exhausted = Harness::new();
    exhausted.queue("a|b");
    exhausted.queue("c|-");
    let prefix_result = prefix(exhausted.core.clone(), PageEndpoint::new("a0"), 2)
        .await
        .expect("exact exhaustion");
    assert_eq!(prefix_result.items(), ["a", "c"]);
    assert_eq!(prefix_result.summary().stop_reason(), StopReason::Exhausted);
    assert_eq!(
        prefix_result.summary().continuation(),
        ContinuationKnowledge::Exhausted
    );
    assert_eq!(exhausted.requests(), 2);

    let bounded = Harness::new();
    bounded.queue("a|b");
    bounded.queue("c|d");
    let prefix_result = prefix(bounded.core.clone(), PageEndpoint::new("a0"), 2)
        .await
        .expect("bounded prefix");
    assert_eq!(prefix_result.items(), ["a", "c"]);
    let summary = prefix_result.summary();
    assert_eq!(summary.stop_reason(), StopReason::PageBound);
    assert_eq!(
        summary.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    assert_eq!(summary.pages_received(), 2);
    assert_eq!(summary.pages_delivered(), 2);
    assert_eq!(bounded.requests(), 2, "no extra fetch at the bound");
    // Three identities: page 0, page 1 and the validated page-2 request.
    assert!(summary.retained_identity_bytes() > 0);
}

/// V3-P05: complete collection returns a Vec only after `Done`; prefix and
/// lazy summaries distinguish the bound, an application stop and exhaustion.
#[tokio::test]
async fn complete_prefix_and_lazy_terminals_report_distinct_truths() {
    let complete = Harness::new();
    complete.queue("a|b");
    complete.queue("c|-");
    assert_eq!(
        all(complete.core.clone(), PageEndpoint::new("a0"), 2)
            .await
            .expect("complete after Done"),
        ["a", "c"]
    );

    let limited = Harness::new();
    limited.queue("a|b");
    limited.queue("c|d");
    let endpoint = PageEndpoint::new("a0");
    let calls = endpoint.calls.clone();
    let error = all(limited.core.clone(), endpoint, 2)
        .await
        .expect_err("completion limit is never a partial success");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CompletionLimitExceeded)
    );
    assert_eq!(
        error.failure_stage(),
        crate::error::FailureStage::PaginationDecision
    );
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::PageBound);
    assert_eq!(
        summary.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    assert_eq!(summary.pages_received(), 2);
    assert_eq!(summary.pages_delivered(), 1);
    assert_eq!(Calls::get(&calls.advances), 2);
    assert_eq!(limited.requests(), 2);
    let aggregate = limited.hooks.aggregate();
    assert_eq!(aggregate[0].stop_reason, StopReason::PageBound);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Pagination);

    let lazy = Harness::new();
    lazy.queue("a|b");
    lazy.queue("c|d");
    let mut pages: Pages<String> = pages_request(lazy.request(PageEndpoint::new("a0")), 5);
    let page = pages
        .next_page()
        .await
        .expect("first lazy page")
        .expect("delivered");
    assert_eq!(page.index(), 0);
    assert_eq!(page.items(), ["a"]);
    let active = pages.summary();
    assert_eq!(active.stop_reason(), StopReason::Active);
    assert_eq!(
        active.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    let finished = pages.finish();
    assert_eq!(finished.stop_reason(), StopReason::Application);
    assert_eq!(finished.pages_delivered(), 1);
    assert_eq!(lazy.requests(), 1, "no background prefetch");
    let aggregate = lazy.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].stop_reason, StopReason::Application);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Success);
}

/// §23.4: an application search that stops early reports `Application`, a
/// page bound is not "not found", and exhaustion is only reported on `Done`.
#[tokio::test]
async fn application_search_distinguishes_found_bound_and_exhaustion() {
    async fn find(harness: &Harness, wanted: &str, bound: u32) -> (bool, TraversalSummary) {
        let mut pages: Pages<String> =
            pages_request(harness.request(PageEndpoint::new("s0")), bound);
        while let Some(page) = pages.next_page().await.expect("search page") {
            if page.into_items().iter().any(|item| item == wanted) {
                return (true, pages.finish());
            }
        }
        (false, pages.finish())
    }

    let found = Harness::new();
    found.queue("x|s1");
    found.queue("wanted|s2");
    let (hit, summary) = find(&found, "wanted", 10).await;
    assert!(hit);
    assert_eq!(summary.stop_reason(), StopReason::Application);

    let bounded = Harness::new();
    bounded.queue("x|s1");
    let (hit, summary) = find(&bounded, "wanted", 1).await;
    assert!(!hit);
    assert_eq!(summary.stop_reason(), StopReason::PageBound);

    let exhausted = Harness::new();
    exhausted.queue("x|-");
    let (hit, summary) = find(&exhausted, "wanted", 10).await;
    assert!(!hit);
    assert_eq!(summary.stop_reason(), StopReason::Exhausted);
}

/// V3-P06 / PAGE-03: one absolute deadline spans every page and the time the
/// application spends between polls; it is never reset per page.
#[tokio::test(start_paused = true)]
async fn aggregate_deadline_spans_caller_pauses() {
    let harness = Harness::new();
    harness.queue("a|b");
    harness.queue("unused|-");
    let endpoint = PageEndpoint::new("a0").timeout(Duration::from_secs(10));
    let mut pages: Pages<String> = pages_request(harness.request(endpoint), 5);

    // Construction is inert: time passing before the first poll is free.
    tokio::time::advance(Duration::from_secs(60)).await;
    let first = pages
        .next_page()
        .await
        .expect("first page within the deadline")
        .expect("delivered");
    assert_eq!(first.into_items(), ["a"]);

    // The application pauses between polls for longer than the deadline.
    tokio::time::advance(Duration::from_secs(11)).await;
    let error = pages.next_page().await.expect_err("aggregate deadline");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(harness.requests(), 1, "the expired page is never sent");
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(summary.pages_delivered(), 1);
    assert!(
        pages
            .next_page()
            .await
            .expect("error returned once")
            .is_none()
    );

    // A selected timeout on a collector is one aggregate deadline too.
    let collector = Harness::new();
    collector.queue("a|b");
    let control = collect_all_request(
        collector.request(PageEndpoint::new("a0")),
        NonZeroU32::new(5).expect("nonzero"),
    )
    .timeout(Duration::from_secs(3))
    .expect("valid timeout");
    assert_eq!(
        control.control_summary().effective_timeout(),
        Some(Duration::from_secs(3))
    );
}

/// V3-P06: caller send and replay caps count across pages and retries and
/// never reset per page.
#[tokio::test]
async fn send_and_replay_budgets_persist_across_pages_and_retries() {
    // Send cap: two handoffs cover two first-page submissions only.
    let sends = Harness::new();
    sends.queue("a|b");
    sends.queue("c|d");
    sends.queue("unused|-");
    let error = collect_all_request(
        sends.request(PageEndpoint::new("a0")),
        NonZeroU32::new(5).expect("nonzero"),
    )
    .with_options(&CallOptions::new().cap_sends(2))
    .await
    .expect_err("send cap");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::SendCapExhausted)
    );
    assert_eq!(sends.requests(), 2);
    let summary = summary_of(&error);
    assert_eq!(summary.application_attempts(), 2);
    assert_eq!(summary.pages_received(), 2);

    // A zero send cap stops before any preparation or controller start.
    let zero = Harness::new();
    let endpoint = PageEndpoint::new("a0");
    let calls = endpoint.calls.clone();
    let error = collect_prefix_request(zero.request(endpoint), 3)
        .with_options(&CallOptions::new().cap_sends(0))
        .await
        .expect_err("zero send cap");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::SendCapExhausted)
    );
    assert_eq!(Calls::get(&calls.starts), 0);
    assert_eq!(zero.requests(), 0);

    // Retries consume the same send budget as later pages.
    let policy = || {
        RetryPolicy::builder()
            .max_retries_per_operation(2)
            .without_budget()
            .classify_fn(|attempt| match attempt.failure() {
                RetryFailure::RejectedStatus {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    ..
                } => RetryDecision::Retry,
                _ => RetryDecision::Stop,
            })
            .build()
            .expect("retry policy")
    };
    let retried = Harness::configured(|runtime| runtime.retry_policy(policy()));
    retried.queue_status(StatusCode::SERVICE_UNAVAILABLE, "");
    retried.queue("a|b");
    retried.queue("unused|-");
    let error = collect_all_request(
        retried.request(PageEndpoint::new("a0")),
        NonZeroU32::new(5).expect("nonzero"),
    )
    .with_options(&CallOptions::new().cap_sends(2))
    .await
    .expect_err("retry spent the send budget");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::SendCapExhausted)
    );
    assert_eq!(retried.requests(), 2);
    assert_eq!(summary_of(&error).automatic_replays(), 1);

    // Replay cap: one replay total across the whole traversal.
    let replays = Harness::configured(|runtime| runtime.retry_policy(policy()));
    replays.queue_status(StatusCode::SERVICE_UNAVAILABLE, "");
    replays.queue("a|b");
    replays.queue_status(StatusCode::SERVICE_UNAVAILABLE, "replay-denied-body");
    replays.queue("unused|-");
    let error = collect_all_request(
        replays.request(PageEndpoint::new("a0")),
        NonZeroU32::new(5).expect("nonzero"),
    )
    .with_options(&CallOptions::new().cap_replays(1))
    .await
    .expect_err("second page cannot replay");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(replays.requests(), 3);
    let summary = summary_of(&error);
    assert_eq!(summary.automatic_replays(), 1);
    assert_eq!(summary.application_attempts(), 3);
    assert_eq!(summary.pages_received(), 1);
}

/// V3-P06 / PAGE-04: acquired bytes include failed partial acquisitions and
/// persist across pages.
#[tokio::test]
async fn acquired_bytes_include_failed_partial_acquisitions() {
    const FIRST: &str = "a|b";
    const OVERSIZED: &str = "0123456789abcdef-oversized|-";
    let harness = Harness::configured(|runtime| runtime.response_body_limit(8));
    harness.queue(FIRST);
    // An observed (streamed) body has no known length, so no Content-Length
    // preflight rejects it: its bytes are read before the limit fails.
    let polls = Arc::new(AtomicUsize::new(0));
    harness
        .executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(OVERSIZED.as_bytes()))
                .body_poll_counter(Arc::clone(&polls)),
        )
        .expect("streamed oversized page");
    let error = prefix(harness.core.clone(), PageEndpoint::new("a0"), 5)
        .await
        .expect_err("second page exceeds the per-response limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    let summary = summary_of(&error);
    assert_eq!(
        summary.acquired_response_bytes(),
        u64::try_from(FIRST.len() + OVERSIZED.len()).expect("small"),
        "bytes actually read before the failure are counted"
    );
    assert!(polls.load(Ordering::SeqCst) >= 1);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
}

/// V3-P07: dropping an unpolled fetch changes nothing.
#[tokio::test]
async fn dropping_an_unpolled_next_page_future_changes_nothing() {
    let harness = Harness::new();
    harness.queue("a|-");
    let mut pages: Pages<String> = pages_request(harness.request(PageEndpoint::new("a0")), 2);
    drop(pages.next_page());
    assert_eq!(pages.summary().stop_reason(), StopReason::NotStarted);
    assert_eq!(harness.requests(), 0);
    let page = pages
        .next_page()
        .await
        .expect("later fetch works")
        .expect("delivered");
    assert_eq!(page.items(), ["a"]);
    assert_eq!(pages.finish().stop_reason(), StopReason::Exhausted);
}

/// V3-P07: dropping a started fetch terminates the traversal; later fetches
/// return `Ok(None)` and the summary retains `Cancelled`.
#[tokio::test]
async fn dropping_a_started_fetch_cancels_the_traversal() {
    let harness = Harness::new();
    let gate = Gate::closed();
    harness.queue("a|b");
    harness
        .executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"c|d")).delivery_gate(gate.clone()),
        )
        .expect("gated page");
    harness.queue("unused|-");
    let mut pages: Pages<String> = pages_request(harness.request(PageEndpoint::new("a0")), 5);
    pages
        .next_page()
        .await
        .expect("first page")
        .expect("delivered");
    {
        let mut next = pin!(pages.next_page());
        assert!(poll_once(next.as_mut()).is_pending());
        gate.wait_until_reached().await;
    }
    gate.open();
    let summary = pages.summary();
    assert_eq!(summary.stop_reason(), StopReason::Cancelled);
    assert_eq!(summary.pages_delivered(), 1);
    assert_eq!(summary.application_attempts(), 2);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(pages.next_page().await.expect("cancelled").is_none());
    assert!(pages.next_page().await.expect("no replenishment").is_none());
    assert_eq!(pages.finish().stop_reason(), StopReason::Cancelled);
    assert_eq!(harness.requests(), 2);
    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].stop_reason, StopReason::Cancelled);
    assert_eq!(aggregate[0].outcome, CompletionOutcome::Cancelled);
}

/// V3-P07: an attached cancellation signal ends an in-flight fetch with one
/// error and a `Cancelled` summary.
#[tokio::test]
async fn cancellation_signal_terminates_an_in_flight_fetch_once() {
    let harness = Harness::new();
    let gate = Gate::closed();
    harness
        .executor
        .application()
        .queue_response(
            ScriptedResponse::success(Bytes::from_static(b"a|b")).delivery_gate(gate.clone()),
        )
        .expect("gated page");
    let token = CancellationToken::new();
    let request = harness.request(PageEndpoint::new("a0"));
    let mut request = request;
    request
        .options
        .add_cancellation(token.clone())
        .expect("cancellation source");
    let mut pages: Pages<String> = pages_request(request, 5);
    let canceller = {
        let token = token.clone();
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.wait_until_reached().await;
            token.cancel();
        })
    };
    let error = pages.next_page().await.expect_err("cancelled fetch");
    canceller.await.expect("canceller");
    gate.open();
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Cancelled)
    );
    let summary = summary_of(&error);
    assert_eq!(summary.stop_reason(), StopReason::Cancelled);
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    assert!(pages.next_page().await.expect("returned once").is_none());
    assert_eq!(pages.finish().stop_reason(), StopReason::Cancelled);

    // A signal cancelled before the first poll prevents all work.
    let early = Harness::new();
    let token = CancellationToken::new();
    token.cancel();
    let endpoint = PageEndpoint::new("a0");
    let calls = endpoint.calls.clone();
    let error = collect_prefix_request(early.request(endpoint), 3)
        .cancel_with(token)
        .expect("cancellation source")
        .await
        .expect_err("already cancelled");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::Cancelled)
    );
    assert_eq!(summary_of(&error).operation_id(), None);
    assert_eq!(Calls::get(&calls.starts), 0);
    assert_eq!(early.requests(), 0);
}

/// V3-P07: a fetch error is returned exactly once.
#[tokio::test]
async fn fetch_error_is_returned_once_then_none_with_failed() {
    let harness = Harness::new();
    harness.queue("a|b");
    harness.queue_status(StatusCode::INTERNAL_SERVER_ERROR, "");
    harness.queue("unused|-");
    let mut pages: Pages<String> = pages_request(harness.request(PageEndpoint::new("a0")), 5);
    assert!(pages.next_page().await.expect("first").is_some());
    let error = pages.next_page().await.expect_err("second fails");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(pages.next_page().await.expect("after error").is_none());
    let summary = pages.finish();
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert_eq!(harness.requests(), 2);
}

/// V3-P07: dropping a started traversal between fetches records cancellation.
#[tokio::test]
async fn dropping_a_started_traversal_records_cancellation() {
    let harness = Harness::new();
    harness.queue("a|b");
    let mut pages: Pages<String> = pages_request(harness.request(PageEndpoint::new("a0")), 5);
    assert!(pages.next_page().await.expect("first").is_some());
    drop(pages);
    let aggregate = harness.hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].stop_reason, StopReason::Cancelled);
    assert_eq!(aggregate[0].pages_completed, 1);
    assert_eq!(harness.requests(), 1);
}

/// Pages are only offered for safe endpoints; a forged bridge cannot page an
/// unsafe endpoint.
#[tokio::test]
async fn unsafe_effect_is_ineligible_before_any_work() {
    let harness = Harness::new();
    let mut endpoint = PageEndpoint::new("a0");
    endpoint.effect = Effect::NonIdempotent;
    let calls = endpoint.calls.clone();
    let error = prefix(harness.core.clone(), endpoint, 2)
        .await
        .expect_err("ineligible endpoint");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::IneligibleEndpoint)
    );
    assert_eq!(Calls::get(&calls.starts), 0);
    assert_eq!(harness.requests(), 0);
}

#[test]
fn collector_controls_validate_and_compose() {
    let harness = Harness::new();
    let collector = collect_prefix_request(harness.request(PageEndpoint::new("a0")), 2);
    assert!(matches!(
        collector.timeout(Duration::ZERO),
        Err(ControlError::ZeroDuration)
    ));

    let collector = collect_all_request(harness.request(PageEndpoint::new("a0")), NonZeroU32::MIN)
        .with_options(
            &CallOptions::new()
                .cap_sends(7)
                .cap_timeout(Duration::from_secs(5))
                .expect("cap"),
        )
        .timeout(Duration::from_secs(60))
        .expect("selected timeout");
    let summary = collector.control_summary();
    assert_eq!(summary.send_cap(), Some(7));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(summary.declared_timeout(), Some(Duration::from_secs(30)));
    let summary = collector.reset_timeout().control_summary();
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(5)));

    fn generic<R: RequestControls>(request: R, token: CancellationToken) -> R {
        request.cancel_with(token).expect("within bound")
    }
    let token = CancellationToken::new();
    let mut collector = generic(
        collect_prefix_request(harness.request(PageEndpoint::new("a0")), 2),
        token.clone(),
    );
    collector = generic(collector, token);
    assert_eq!(collector.control_summary().cancellation_sources(), 1);
    for _ in 1..crate::controls::MAX_CANCELLATION_SOURCES {
        collector = collector
            .cancel_with(CancellationToken::new())
            .expect("within bound");
    }
    assert!(matches!(
        collector.cancel_with(CancellationToken::new()),
        Err(ControlError::TooManyCancellationSources)
    ));
    assert!(
        !format!(
            "{:?}",
            collect_prefix_request(harness.request(PageEndpoint::new("debug-sentinel-7a")), 1)
        )
        .contains("debug-sentinel-7a")
    );
    assert_eq!(harness.requests(), 0);
}
