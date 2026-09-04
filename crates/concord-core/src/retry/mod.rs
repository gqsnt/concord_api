//! Concord-owned managed retry policy and bounded budget primitives.

mod budget;
mod policy;
mod server_delay;

pub(crate) use server_delay::{ServerDelayHint, parse_server_delay};

pub use policy::{
    MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryBackoff, RetryBackoffError,
    RetryClassifier, RetryDecision, RetryFailure, RetryJitter, RetryPolicy, RetryPolicyBuildError,
    RetryPolicyBuildErrorKind, RetryPolicyBuilder,
};

#[cfg(test)]
mod tests;
