//! Phase-6 final credential freshness (bounded restarts, committed admission
//! accounting) and the zero-send fast path before credential work.

use super::*;
use crate::controls::CallOptions;
use crate::error::{DispatchKnowledge, FailureStage};

/// A limiter whose every admission installs a newer credential version, so
/// the phase-6 freshness check always finds the admitted material changed.
struct ChurningLimiter {
    selector: Arc<SlotSelector<AccessToken>>,
    acquisitions: Arc<AtomicUsize>,
}

impl RateLimiter for ChurningLimiter {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "test.churning-limiter",
            1,
        )
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let generation = self.acquisitions.fetch_add(1, Ordering::SeqCst);
        self.selector
            .replace(access_token(format!("churned-token-{generation}")))
            .expect("churning replacement");
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, _response: RateLimitResponse<'_>) {}
}

#[tokio::test]
async fn phase6_repeated_credential_churn_is_credential_unavailable_with_admission_counts() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(ChurningLimiter {
            selector: Arc::clone(&selector),
            acquisitions: Arc::clone(&acquisitions),
        })),
    );

    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            PreparedBody::bytes(Bytes::new()),
        )
        .await,
    );

    // Initial admission plus exactly two freshness restarts: at most three
    // committed admissions per allowed handoff, then a bounded failure.
    assert_eq!(acquisitions.load(Ordering::SeqCst), 3);
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Unavailable));
    assert_eq!(error.failure_stage(), FailureStage::BeforeDispatch);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(!error.may_have_reached_server());
    let counts = error.operation_counts().expect("journal counts");
    assert_eq!(counts.quota_admissions(), 3);
    assert_eq!(counts.freshness_restarts(), 2);
    assert_eq!(counts.application_handoffs(), 0);
    // Restarts never inflate attempt counts.
    assert_eq!(counts.attempts_started(), 1);
    let last = error.last_attempt().expect("last attempt");
    assert_eq!(last.index(), 0);
    assert_eq!(last.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let completions = hooks.completion.lock().expect("completion records");
    assert_eq!(completions.len(), 1);
    assert_eq!(
        completions[0].5,
        CompletionOutcome::AuthenticationUnavailable
    );
}

#[tokio::test]
async fn zero_send_cap_performs_no_body_or_credential_preflight_work() {
    let executor = DeterministicExecutor::new();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&acquires),
        },
        Duration::ZERO,
    );
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::counting(
            &limiter_acquires,
            &limiter_observations,
        ))),
    );
    let body = PreparedBody::factory(ReplayFactory {
        calls: Arc::clone(&factory_calls),
    });

    let error = api_error(
        request(core, &selector, ChallengePolicy::Never, body)
            .with_options(&CallOptions::new().cap_sends(0))
            .await,
    );

    assert_eq!(error.category(), ErrorCategory::SendCapExhausted);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(error.failure_stage(), FailureStage::BeforeDispatch);
    assert_eq!(error.operation_id(), None);
    assert_eq!(acquires.load(Ordering::SeqCst), 0);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert!(
        hooks
            .completion
            .lock()
            .expect("completion records")
            .is_empty()
    );
}

/// CONTROL-02: a selected timeout that is a valid control value but cannot be
/// represented as an absolute monotonic deadline from the first poll is
/// rejected at that poll, before any effect: `InvalidValue`, `NotDispatched`,
/// and no body, provider, limiter, attempt or transport work.
#[tokio::test]
async fn unrepresentable_deadline_is_rejected_at_first_poll_before_effects() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&acquires),
        },
        Duration::ZERO,
    );
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::counting(
            &limiter_acquires,
            &limiter_observations,
        ))),
    );
    let body = PreparedBody::factory(ReplayFactory {
        calls: Arc::clone(&factory_calls),
    });
    let margin = Duration::from_secs(60 * 60);
    // Accepted as a control: finite, positive and below the control maximum.
    let request = request(core, &selector, ChallengePolicy::Never, body)
        .timeout(margin * 2)
        .expect("a representable control duration");
    crate::operation::TestClockGuard::advance_to_representable_limit(margin);

    let error = api_error(request.await);

    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidValue)
    );
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(error.failure_stage(), FailureStage::Preparation);
    assert!(error.last_attempt().is_none());
    assert_eq!(acquires.load(Ordering::SeqCst), 0);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(executor.provider().request_count().expect("count"), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert!(hooks.response.lock().expect("response records").is_empty());
}
