//! Explicit Concord-managed retry policy.
//!
//! Required features: none. Mode: construction-only. Documentation owners:
//! `docs/features/retries.md` and `docs/recipes/retries.md`.

use concord::api;
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};

api! {
    client RetryApi {
        base "https://retry.example.test/";
    }

    GET health() -> Bytes;
}

/// Builds a finite policy for transient connection, transport, and selected
/// rejected-status failures.
// ANCHOR: managed_retry_policy
pub fn managed_retry_policy() -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(2)
        .max_extra_load(0.20)
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::Connect | RetryFailure::Transport => RetryDecision::Retry,
            RetryFailure::RejectedStatus { status, .. }
                if status == concord::http::StatusCode::TOO_MANY_REQUESTS
                    || status.is_server_error() =>
            {
                RetryDecision::Retry
            }
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("the documented retry policy is structurally valid")
}
// ANCHOR_END: managed_retry_policy

/// Installs policy through the generated client builder.
// ANCHOR: managed_retry_client
pub fn construct() -> Result<RetryApi, concord::prelude::ClientBuildError> {
    RetryApi::builder().retry(managed_retry_policy()).build()
}
// ANCHOR_END: managed_retry_client
