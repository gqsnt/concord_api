//! Concord-owned managed retry policy and bounded budget primitives.

mod budget;
mod policy;

pub use policy::{
    MAX_POLICY_RETRIES_PER_OPERATION, RetryAttempt, RetryClassifier, RetryDecision, RetryFailure,
    RetryPolicy, RetryPolicyBuildError, RetryPolicyBuildErrorKind, RetryPolicyBuilder,
};

#[cfg(test)]
mod tests;
