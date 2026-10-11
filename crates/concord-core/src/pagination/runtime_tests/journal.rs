//! Traversal errors carry journal context (EXEC-01/EXEC-02): a page failure
//! keeps its page operation, parent traversal and last-attempt facts, and the
//! traversal summary joins the aggregate dispatch knowledge.

use super::*;
use crate::error::{DispatchKnowledge, FailureStage};

#[tokio::test]
async fn page_failure_keeps_page_journal_with_parent_traversal_and_joined_aggregate() {
    let harness = Harness::new();
    harness.queue("first|second");
    harness.queue_status(StatusCode::SERVICE_UNAVAILABLE, "unavailable");
    let error = prefix(harness.core.clone(), PageEndpoint::new("initial"), 5)
        .await
        .expect_err("second page is rejected");

    let summary = summary_of(&error);
    let traversal = summary.operation_id().expect("started traversal");
    assert_eq!(error.parent_operation_id(), Some(traversal));
    let page = error.operation_id().expect("page operation");
    assert_ne!(page, traversal);
    assert_eq!(error.page_index(), Some(1));
    assert_eq!(error.failure_stage(), FailureStage::RejectedResponse);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(
        error.last_response_status(),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    let last = error.last_attempt().expect("page attempt");
    assert_eq!(last.index(), 0);
    assert_eq!(last.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        summary.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
}

#[tokio::test]
async fn traversal_decision_failure_uses_the_traversal_journal() {
    let harness = Harness::new();
    // The continuation repeats the initial request: a decision failure after
    // one observed page.
    harness.queue("only|initial");
    let error = prefix(harness.core.clone(), PageEndpoint::new("initial"), 5)
        .await
        .expect_err("repeated continuation");

    let summary = summary_of(&error);
    assert_eq!(error.operation_id(), summary.operation_id());
    assert_eq!(error.parent_operation_id(), None);
    assert_eq!(error.failure_stage(), FailureStage::PaginationDecision);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(error.may_have_reached_server());
}
