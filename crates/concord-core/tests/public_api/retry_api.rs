use std::error::Error;
use std::time::Duration;

use concord_core::contracts::{
    MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryClassifier, RetryDecision, RetryFailure,
    RetryPolicy, RetryPolicyBuildErrorKind,
};

struct GetFailures;

impl RetryClassifier for GetFailures {
    fn classify(&self, attempt: RetryAttempt<'_>) -> RetryDecision {
        let _ = (
            attempt.endpoint(),
            attempt.method(),
            attempt.host(),
            attempt.page_index(),
            attempt.attempt(),
            attempt.retries_performed(),
        );
        match attempt.failure() {
            RetryFailure::Connect | RetryFailure::Transport => RetryDecision::Retry,
            RetryFailure::RejectedStatus {
                status,
                server_delay,
            } => {
                let _ = (status, server_delay);
                RetryDecision::Stop
            }
            _ => RetryDecision::Stop,
        }
    }
}

#[test]
fn downstream_retry_policy_uses_only_semantic_contracts() {
    let policy = RetryPolicy::builder()
        .max_extra_load(0.25)
        .max_retries_per_operation(MAX_POLICY_RETRIES_PER_OPERATION)
        .budget_scope_capacity(7)
        .budget_scope_ttl(Duration::from_secs(11))
        .classify(GetFailures)
        .build()
        .expect("bounded retry policy");
    assert!(format!("{policy:?}").contains("RetryPolicy"));

    let closure_policy = RetryPolicy::builder()
        .without_budget()
        .classify_fn(|_| RetryDecision::Stop)
        .build()
        .expect("closure retry policy");
    let _ = closure_policy.clone();

    let missing = RetryPolicy::builder()
        .build()
        .expect_err("classifier is required");
    assert_eq!(missing.kind(), RetryPolicyBuildErrorKind::MissingClassifier);
    assert!(Error::source(&missing).is_none());
}
