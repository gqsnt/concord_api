use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use http::{Method, StatusCode};
use url::Url;

use crate::error::{DispatchKnowledge, FailureStage};
use crate::hooks::AttemptInfo;
use crate::redaction::SanitizedHeaders;

use super::budget::{FixedRatio, RetryBudget};

const DEFAULT_MAX_EXTRA_LOAD: f64 = 0.20;
const DEFAULT_MAX_RETRIES_PER_OPERATION: u32 = 2;
const DEFAULT_BUDGET_SCOPE_CAPACITY: usize = 4_096;
const DEFAULT_BUDGET_SCOPE_TTL: Duration = Duration::from_secs(15 * 60);

/// Hard maximum number of managed policy retries for one request or page.
pub const MAX_POLICY_RETRIES_PER_OPERATION: u32 = 16;

/// Shared Concord-managed retry policy.
///
/// Clones share classifier and rolling-budget state. Policies built
/// independently retain isolated state.
#[derive(Clone)]
pub struct RetryPolicy {
    inner: Arc<RetryPolicyInner>,
}

impl RetryPolicy {
    /// Starts a retry-policy builder with bounded defaults and no classifier.
    #[must_use]
    pub fn builder() -> RetryPolicyBuilder {
        RetryPolicyBuilder::new()
    }

    pub(crate) fn classify(&self, attempt: RetryAttempt<'_>) -> RetryDecision {
        self.inner.classifier.classify(attempt)
    }

    pub(crate) fn max_retries_per_operation(&self) -> u32 {
        self.inner.max_retries_per_operation
    }

    pub(crate) fn record_initial_dispatch(&self, url: &Url) {
        if let Some(budget) = &self.inner.budget {
            budget.record_initial_dispatch(url);
        }
    }

    pub(crate) fn reserve_retry(&self, url: &Url) -> bool {
        self.inner
            .budget
            .as_ref()
            .is_none_or(|budget| budget.reserve_retry(url))
    }

    #[cfg(test)]
    pub(super) fn budget(&self) -> Option<&RetryBudget> {
        self.inner.budget.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn poison_budget_state_for_test(&self) -> bool {
        self.inner
            .budget
            .as_ref()
            .is_some_and(RetryBudget::poison_state_for_test)
    }

    #[cfg(test)]
    pub(super) fn shares_state_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for RetryPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetryPolicy")
            .field(
                "max_retries_per_operation",
                &self.inner.max_retries_per_operation,
            )
            .field("budget_enabled", &self.inner.budget.is_some())
            .field("classifier", &"<application classifier omitted>")
            .finish()
    }
}

struct RetryPolicyInner {
    classifier: Arc<dyn RetryClassifier>,
    max_retries_per_operation: u32,
    budget: Option<RetryBudget>,
}

/// Builder for one explicit managed retry policy.
pub struct RetryPolicyBuilder {
    max_extra_load: f64,
    max_retries_per_operation: u32,
    budget_enabled: bool,
    budget_scope_capacity: usize,
    budget_scope_ttl: Duration,
    classifier: Option<Arc<dyn RetryClassifier>>,
}

impl RetryPolicyBuilder {
    fn new() -> Self {
        Self {
            max_extra_load: DEFAULT_MAX_EXTRA_LOAD,
            max_retries_per_operation: DEFAULT_MAX_RETRIES_PER_OPERATION,
            budget_enabled: true,
            budget_scope_capacity: DEFAULT_BUDGET_SCOPE_CAPACITY,
            budget_scope_ttl: DEFAULT_BUDGET_SCOPE_TTL,
            classifier: None,
        }
    }

    /// Replaces the rolling-budget target retry-to-initial-dispatch ratio.
    ///
    /// For one logical origin in the active ten-second window:
    ///
    /// `allowed retries = ceil(initial dispatches × ratio)`.
    ///
    /// A ratio of `0.20` permits one retry after one initial dispatch.
    /// Whole-retry upward rounding can make the budget allowance ratio exceed
    /// the configured ratio at low volume. The allowance ratio converges
    /// toward the configured value as initial dispatch volume grows. Actual
    /// approved retries may be lower because classifiers can stop, bodies can
    /// be non-replayable, budget admission can deny, and per-operation limits
    /// can intervene.
    ///
    /// Values must be finite and within `0.0..=1000.0`.
    #[must_use]
    pub fn max_extra_load(mut self, ratio: f64) -> Self {
        self.max_extra_load = ratio;
        self
    }

    /// Replaces the per-operation policy-retry maximum.
    ///
    /// Values must be within `1..=MAX_POLICY_RETRIES_PER_OPERATION`.
    #[must_use]
    pub const fn max_retries_per_operation(mut self, maximum: u32) -> Self {
        self.max_retries_per_operation = maximum;
        self
    }

    /// Disables shared rolling-budget admission.
    ///
    /// The hard per-operation retry maximum remains enforced.
    #[must_use]
    pub const fn without_budget(mut self) -> Self {
        self.budget_enabled = false;
        self
    }

    /// Replaces the positive maximum number of retained origin scopes.
    #[must_use]
    pub const fn budget_scope_capacity(mut self, capacity: usize) -> Self {
        self.budget_scope_capacity = capacity;
        self
    }

    /// Replaces the positive idle TTL for retained origin scopes.
    #[must_use]
    pub const fn budget_scope_ttl(mut self, ttl: Duration) -> Self {
        self.budget_scope_ttl = ttl;
        self
    }

    /// Selects an application retry classifier.
    #[must_use]
    pub fn classify<C>(mut self, classifier: C) -> Self
    where
        C: RetryClassifier,
    {
        self.classifier = Some(Arc::new(classifier));
        self
    }

    /// Selects a closure as the application retry classifier.
    #[must_use]
    pub fn classify_fn<F>(self, classifier: F) -> Self
    where
        F: Fn(RetryAttempt<'_>) -> RetryDecision + Send + Sync + 'static,
    {
        self.classify(FunctionClassifier(classifier))
    }

    /// Validates and builds the shared retry policy.
    pub fn build(self) -> Result<RetryPolicy, RetryPolicyBuildError> {
        let ratio = FixedRatio::from_f64(self.max_extra_load).ok_or_else(|| {
            RetryPolicyBuildError::new(RetryPolicyBuildErrorKind::InvalidMaxExtraLoad)
        })?;
        if !(1..=MAX_POLICY_RETRIES_PER_OPERATION).contains(&self.max_retries_per_operation) {
            return Err(RetryPolicyBuildError::new(
                RetryPolicyBuildErrorKind::InvalidMaxRetriesPerOperation,
            ));
        }
        if self.budget_scope_capacity == 0 {
            return Err(RetryPolicyBuildError::new(
                RetryPolicyBuildErrorKind::InvalidBudgetScopeCapacity,
            ));
        }
        if self.budget_scope_ttl.is_zero() {
            return Err(RetryPolicyBuildError::new(
                RetryPolicyBuildErrorKind::InvalidBudgetScopeTtl,
            ));
        }
        let classifier = self.classifier.ok_or_else(|| {
            RetryPolicyBuildError::new(RetryPolicyBuildErrorKind::MissingClassifier)
        })?;
        let budget = self
            .budget_enabled
            .then(|| RetryBudget::new(ratio, self.budget_scope_capacity, self.budget_scope_ttl));
        Ok(RetryPolicy {
            inner: Arc::new(RetryPolicyInner {
                classifier,
                max_retries_per_operation: self.max_retries_per_operation,
                budget,
            }),
        })
    }
}

impl fmt::Debug for RetryPolicyBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetryPolicyBuilder")
            .field("max_extra_load", &self.max_extra_load)
            .field("max_retries_per_operation", &self.max_retries_per_operation)
            .field("budget_enabled", &self.budget_enabled)
            .field("budget_scope_capacity", &self.budget_scope_capacity)
            .field("budget_scope_ttl", &self.budget_scope_ttl)
            .field("classifier_configured", &self.classifier.is_some())
            .finish()
    }
}

struct FunctionClassifier<F>(F);

impl<F> RetryClassifier for FunctionClassifier<F>
where
    F: Fn(RetryAttempt<'_>) -> RetryDecision + Send + Sync + 'static,
{
    fn classify(&self, attempt: RetryAttempt<'_>) -> RetryDecision {
        (self.0)(attempt)
    }
}

/// Synchronous application classification of retryable managed failures.
pub trait RetryClassifier: Send + Sync + 'static {
    /// Returns whether Concord should request another managed attempt.
    fn classify(&self, attempt: RetryAttempt<'_>) -> RetryDecision;
}

/// Safe borrowed context for one failed visible managed attempt.
#[derive(Clone, Copy)]
pub struct RetryAttempt<'a> {
    endpoint: &'a str,
    method: &'a Method,
    host: Option<&'a str>,
    page_index: u32,
    attempt: AttemptInfo,
    retries_performed: u32,
    failure: RetryFailure<'a>,
}

impl<'a> RetryAttempt<'a> {
    #[allow(
        clippy::too_many_arguments,
        reason = "the constructor mirrors the finite public classifier context"
    )]
    pub(crate) const fn new(
        endpoint: &'a str,
        method: &'a Method,
        host: Option<&'a str>,
        page_index: u32,
        attempt: AttemptInfo,
        retries_performed: u32,
        failure: RetryFailure<'a>,
    ) -> Self {
        Self {
            endpoint,
            method,
            host,
            page_index,
            attempt,
            retries_performed,
            failure,
        }
    }

    /// Returns the stable generated or runtime endpoint identity.
    #[must_use]
    pub const fn endpoint(&self) -> &'a str {
        self.endpoint
    }

    /// Returns the application HTTP method.
    #[must_use]
    pub const fn method(&self) -> &'a Method {
        self.method
    }

    /// Returns the final logical host when one exists.
    #[must_use]
    pub const fn host(&self) -> Option<&'a str> {
        self.host
    }

    /// Returns the zero-based pagination page index.
    #[must_use]
    pub const fn page_index(&self) -> u32 {
        self.page_index
    }

    /// Returns the structural identity of the failed attempt.
    #[must_use]
    pub const fn attempt(&self) -> AttemptInfo {
        self.attempt
    }

    /// Returns the number of policy retries already performed.
    #[must_use]
    pub const fn retries_performed(&self) -> u32 {
        self.retries_performed
    }

    /// Returns the finite failure offered for classification.
    #[must_use]
    pub const fn failure(&self) -> RetryFailure<'a> {
        self.failure
    }
}

impl fmt::Debug for RetryAttempt<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetryAttempt")
            .field("endpoint", &self.endpoint)
            .field("method", &self.method)
            .field("host_available", &self.host.is_some())
            .field("page_index", &self.page_index)
            .field("attempt", &self.attempt)
            .field("retries_performed", &self.retries_performed)
            .field("failure", &self.failure)
            .finish()
    }
}

/// Failures that an application classifier may consider for managed retry.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub enum RetryFailure<'a> {
    /// Native connection establishment failed.
    Connect,
    /// Another native transport operation failed.
    Transport,
    /// The response status was not accepted by the response specification.
    RejectedStatus {
        /// Rejected final status.
        status: StatusCode,
        /// Sanitized final response headers.
        headers: SanitizedHeaders<'a>,
    },
}

impl RetryFailure<'_> {
    /// Returns the managed failure stage represented by this classifier input.
    #[must_use]
    pub const fn stage(self) -> FailureStage {
        match self {
            Self::Connect => FailureStage::BeforeDispatch,
            Self::Transport => FailureStage::DuringDispatchOrUnknown,
            Self::RejectedStatus { .. } => FailureStage::RejectedResponse,
        }
    }

    /// Returns Concord's dispatch knowledge for this classifier input.
    #[must_use]
    pub const fn dispatch_knowledge(self) -> DispatchKnowledge {
        match self {
            Self::Connect => DispatchKnowledge::NotDispatched,
            Self::Transport => DispatchKnowledge::PossiblyDispatched,
            Self::RejectedStatus { .. } => DispatchKnowledge::ResponseReceived,
        }
    }
}

impl fmt::Debug for RetryFailure<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect => formatter.write_str("Connect"),
            Self::Transport => formatter.write_str("Transport"),
            Self::RejectedStatus { status, headers } => formatter
                .debug_struct("RejectedStatus")
                .field("status", status)
                .field("header_count", &headers.len())
                .finish(),
        }
    }
}

/// Classifier decision for one failed managed attempt.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    /// Stop and preserve the current failure or rejected response.
    Stop,
    /// Request another attempt subject to replay, maximum, and budget checks.
    Retry,
}

/// Stable category for a retry-policy build failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyBuildErrorKind {
    /// A classifier is required.
    MissingClassifier,
    /// The extra-load target ratio was non-finite or outside `0.0..=1000.0`.
    InvalidMaxExtraLoad,
    /// The per-operation maximum was outside the supported range.
    InvalidMaxRetriesPerOperation,
    /// The origin-scope capacity was zero.
    InvalidBudgetScopeCapacity,
    /// The origin-scope idle TTL was zero.
    InvalidBudgetScopeTtl,
}

/// Structural, source-less failure to build a managed retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicyBuildError {
    kind: RetryPolicyBuildErrorKind,
}

impl RetryPolicyBuildError {
    const fn new(kind: RetryPolicyBuildErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable build-failure category.
    #[must_use]
    pub const fn kind(&self) -> RetryPolicyBuildErrorKind {
        self.kind
    }
}

impl fmt::Display for RetryPolicyBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self.kind {
            RetryPolicyBuildErrorKind::MissingClassifier => "a retry classifier is required",
            RetryPolicyBuildErrorKind::InvalidMaxExtraLoad => {
                "extra-load target ratio must be finite and within 0.0 through 1000.0"
            }
            RetryPolicyBuildErrorKind::InvalidMaxRetriesPerOperation => {
                "per-operation retry maximum must be within 1 through 16"
            }
            RetryPolicyBuildErrorKind::InvalidBudgetScopeCapacity => {
                "retry budget scope capacity must be positive"
            }
            RetryPolicyBuildErrorKind::InvalidBudgetScopeTtl => {
                "retry budget scope idle TTL must be positive"
            }
        };
        write!(
            formatter,
            "managed retry policy configuration failed: {detail}"
        )
    }
}

impl Error for RetryPolicyBuildError {}
