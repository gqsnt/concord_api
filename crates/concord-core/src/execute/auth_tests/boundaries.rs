//! V3-E04 (§8.2, §13.1–§13.3, CONTROL-03, EXEC-01/02): a cancellation token
//! and, separately, the operation deadline firing while credential work is
//! waiting — phase-4 credential preflight (a provider acquisition) and the
//! phase-6 final freshness boundary (a provider refresh during finalization,
//! and a freshness restart's new admission).
//!
//! Every case settles before native handoff: the error carries the stage the
//! journal was in, `NotDispatched`, no application handoff and exactly the
//! quota admissions already committed (none before phase 5; a committed
//! phase-5 admission stays consumed, RATE-04). No application request is
//! ever sent.

use std::future::IntoFuture;

use super::*;
use crate::controls::CancellationToken;
use crate::error::{DispatchKnowledge, FailureStage};

/// A provider whose acquisition awaits a test-controlled gate.
struct GatedProvider {
    gate: Gate,
    acquires: Arc<AtomicUsize>,
}

impl CredentialProvider for GatedProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        assert_eq!(context.reason, CredentialReason::Missing);
        self.acquires.fetch_add(1, Ordering::SeqCst);
        let gate = self.gate.clone();
        Box::pin(async move {
            gate.wait().await;
            Ok(access_token(INITIAL))
        })
    }
}

/// A provider whose first material expires while the attempt is admitted
/// (the limiter flips `expired`); its refresh optionally awaits a gate.
struct ExpiringProvider {
    expired: Arc<AtomicBool>,
    refresh_gate: Option<Gate>,
    acquires: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
}

impl CredentialProvider for ExpiringProvider {
    type Material = LimiterWaitExpiryToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquires.fetch_add(1, Ordering::SeqCst);
        let expired = Arc::clone(&self.expired);
        Box::pin(async move {
            Ok(LimiterWaitExpiryToken {
                value: secret_string(INITIAL),
                expired,
            })
        })
    }

    fn refresh<'a>(
        &'a self,
        _current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        assert_eq!(context.reason, CredentialReason::Expired);
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        let gate = self.refresh_gate.clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            Ok(LimiterWaitExpiryToken {
                value: secret_string(REFRESHED),
                expired: Arc::new(AtomicBool::new(false)),
            })
        })
    }
}

/// A limiter whose first admission expires the admitted credential (so the
/// phase-6 freshness check refreshes and restarts), optionally gating the
/// `n`-th admission.
struct ExpiringLimiter {
    expired: Arc<AtomicBool>,
    acquisitions: Arc<AtomicUsize>,
    gate: Option<(usize, Gate)>,
}

impl RateLimiter for ExpiringLimiter {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "test.expiring-limiter",
            1,
        )
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let index = self.acquisitions.fetch_add(1, Ordering::SeqCst);
        if index == 0 {
            self.expired.store(true, Ordering::SeqCst);
        }
        let gate = self
            .gate
            .as_ref()
            .filter(|(gated, _)| *gated == index)
            .map(|(_, gate)| gate.clone());
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            Ok(RateLimitPermit::granted())
        })
    }
}

fn rate_limited_request(
    core: ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
) -> Request<ProtectedEndpoint> {
    Request::new(
        core,
        ProtectedEndpoint {
            credential: CredentialRef::new(Arc::clone(selector)),
            challenge: ChallengePolicy::Never,
            body: PreparedBody::bytes(Bytes::from_static(b"preflight-body")),
            query: false,
            rate_key: Some("boundary-tenant".to_owned()),
        },
    )
}

fn expiring_request(
    core: ClientCore,
    selector: Arc<SlotSelector<LimiterWaitExpiryToken>>,
) -> Request<LimiterWaitExpiryEndpoint> {
    Request::new(
        core,
        LimiterWaitExpiryEndpoint {
            credential: CredentialRef::new(selector),
        },
    )
}

/// Asserts the common pre-handoff facts of a failure fixed while credential
/// work was pending.
fn assert_settled_before_handoff(
    error: &ApiError,
    category: ErrorCategory,
    stage: FailureStage,
    committed_admissions: u32,
    freshness_restarts: u32,
) {
    assert_eq!(error.category(), category);
    assert_eq!(error.failure_stage(), stage);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(!error.may_have_reached_server());
    assert_eq!(
        error.auth_kind(),
        None,
        "not reported as a credential failure"
    );
    assert!(error.operation_id().is_some(), "a started operation");
    let counts = error.operation_counts().expect("journal counts");
    assert_eq!(counts.quota_admissions(), committed_admissions);
    assert_eq!(counts.freshness_restarts(), freshness_restarts);
    assert_eq!(counts.application_handoffs(), 0);
    assert_eq!(counts.attempts_started(), 1);
    let last = error.last_attempt().expect("last attempt");
    assert_eq!(last.index(), 0);
    assert_eq!(last.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(last.status(), None);
}

fn completion_outcomes(hooks: &AuthHooks) -> Vec<CompletionOutcome> {
    hooks
        .completion
        .lock()
        .expect("completion records")
        .iter()
        .map(|record| record.5)
        .collect()
}

#[tokio::test]
async fn v3_e04_cancellation_while_provider_preflight_waits_is_not_dispatched_without_admission() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"after-cancelled-preflight",
        )))
        .expect("response for the later request");
    let gate = Gate::closed();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        GatedProvider {
            gate: gate.clone(),
            acquires: Arc::clone(&acquires),
        },
        Duration::ZERO,
    );
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
    let token = CancellationToken::new();
    let task = tokio::spawn(
        rate_limited_request(core.clone(), &selector)
            .cancel_with(token.clone())
            .expect("one cancellation source")
            .into_future(),
    );
    gate.wait_until_reached().await;
    token.cancel();
    let error = task
        .await
        .expect("request task")
        .expect_err("cancelled during credential preflight");

    assert_settled_before_handoff(
        &error,
        ErrorCategory::Cancelled,
        FailureStage::CredentialPreflight,
        0,
        0,
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0, "no admission");
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(!selector.is_set(), "cancelled acquisition installs nothing");
    assert_eq!(completion_outcomes(&hooks), [CompletionOutcome::Cancelled]);

    // §11.3: the cancelled leader retired its flight, so a later request is
    // not left waiting on it; it leads a new acquisition and is sent.
    gate.open();
    assert_eq!(
        rate_limited_request(core, &selector)
            .await
            .expect("later request after a cancelled preflight"),
        Bytes::from_static(b"after-cancelled-preflight")
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test(start_paused = true)]
async fn v3_e04_deadline_while_provider_preflight_waits_is_a_timeout_not_a_credential_failure() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain unsent");
    let gate = Gate::closed();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        GatedProvider {
            gate: gate.clone(),
            acquires: Arc::clone(&acquires),
        },
        Duration::ZERO,
    );
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
    let started = tokio::time::Instant::now();
    let error = rate_limited_request(core, &selector)
        .timeout(Duration::from_millis(50))
        .expect("valid timeout")
        .await
        .expect_err("operation deadline during credential preflight");
    let elapsed = started.elapsed();
    gate.open();

    assert_settled_before_handoff(
        &error,
        ErrorCategory::Timeout,
        FailureStage::CredentialPreflight,
        0,
        0,
    );
    // The operation deadline fired (not the 30 s provider ceiling).
    assert!(elapsed >= Duration::from_millis(50), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0, "no admission");
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(!selector.is_set());
    assert_eq!(completion_outcomes(&hooks), [CompletionOutcome::Timeout]);
}

#[tokio::test]
async fn v3_e04_cancellation_while_a_phase6_freshness_restart_waits_keeps_the_committed_admission()
{
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain unsent");
    let expired = Arc::new(AtomicBool::new(false));
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        ExpiringProvider {
            expired: Arc::clone(&expired),
            refresh_gate: None,
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
        },
        Duration::ZERO,
    );
    // The restart's new admission (the second acquisition) waits.
    let restart_gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(ExpiringLimiter {
            expired: Arc::clone(&expired),
            acquisitions: Arc::clone(&limiter_acquires),
            gate: Some((1, restart_gate.clone())),
        })),
    );
    let token = CancellationToken::new();
    let task = tokio::spawn(
        expiring_request(core, selector)
            .cancel_with(token.clone())
            .expect("one cancellation source")
            .into_future(),
    );
    restart_gate.wait_until_reached().await;
    token.cancel();
    let error = task
        .await
        .expect("request task")
        .expect_err("cancelled during the freshness restart");
    restart_gate.open();

    // Phase 6 refreshed the expired material and restarted admission once;
    // the first admission was committed and stays consumed, the restart's
    // admission never completed and nothing was handed off.
    assert_settled_before_handoff(
        &error,
        ErrorCategory::Cancelled,
        FailureStage::RateAdmission,
        1,
        1,
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(completion_outcomes(&hooks), [CompletionOutcome::Cancelled]);
}

#[tokio::test(start_paused = true)]
async fn v3_e04_deadline_while_phase6_finalization_refresh_waits_is_a_timeout_before_handoff() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain unsent");
    let expired = Arc::new(AtomicBool::new(false));
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let refresh_gate = Gate::closed();
    let selector = SlotSelector::provider(
        ExpiringProvider {
            expired: Arc::clone(&expired),
            refresh_gate: Some(refresh_gate.clone()),
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
        },
        Duration::ZERO,
    );
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(ExpiringLimiter {
            expired: Arc::clone(&expired),
            acquisitions: Arc::clone(&limiter_acquires),
            gate: None,
        })),
    );
    let started = tokio::time::Instant::now();
    let error = expiring_request(core, selector)
        .timeout(Duration::from_millis(50))
        .expect("valid timeout")
        .await
        .expect_err("operation deadline during the phase-6 refresh");
    let elapsed = started.elapsed();
    refresh_gate.open();

    // The deadline fired while phase-6 finalization waited on the refresh:
    // one committed admission, no restart recorded yet, nothing handed off.
    assert_settled_before_handoff(
        &error,
        ErrorCategory::Timeout,
        FailureStage::BeforeDispatch,
        1,
        0,
    );
    assert!(elapsed >= Duration::from_millis(50), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(completion_outcomes(&hooks), [CompletionOutcome::Timeout]);
}
