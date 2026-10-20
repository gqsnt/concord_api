//! RETRY-02 (§13.5, §11.1): authentication recovery replays only after an
//! authorized provider refresh yields usable material.
//!
//! A provider whose `recovery_mode()` is `None` or `InvalidateOnly` makes an
//! attributed challenge final. There is no refresh and no replay, and the
//! error is `AuthenticationRejected`. `InvalidateOnly` rejects exactly the
//! applied installed version; `None` leaves it installed. A `Refresh`
//! provider whose refresh fails during recovery also never replays. Each case
//! runs on a safe-to-replay request (idempotent effect, replayable body), so
//! only the provider's recovery authority prevents the replay.

use super::*;
use crate::error::{DispatchKnowledge, ProviderAcquisitionOutcome};

/// A provider with a selectable recovery mode whose refresh can fail.
struct ModeProvider {
    mode: RecoveryMode,
    refresh_fails: bool,
    acquires: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
}

impl ModeProvider {
    fn new(
        mode: RecoveryMode,
        refresh_fails: bool,
        acquires: &Arc<AtomicUsize>,
        refreshes: &Arc<AtomicUsize>,
    ) -> Self {
        Self {
            mode,
            refresh_fails,
            acquires: Arc::clone(acquires),
            refreshes: Arc::clone(refreshes),
        }
    }
}

impl CredentialProvider for ModeProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        assert_eq!(context.reason, CredentialReason::Missing);
        let call = self.acquires.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(access_token(if call == 0 { INITIAL } else { NEWER })) })
    }

    fn refresh<'a>(
        &'a self,
        _current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        assert_eq!(context.reason, CredentialReason::Rejected);
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        let fails = self.refresh_fails;
        Box::pin(async move {
            if fails {
                Err(AuthError::new(AuthErrorKind::Acquisition))
            } else {
                Ok(access_token(REFRESHED))
            }
        })
    }

    fn recovery_mode(&self) -> RecoveryMode {
        self.mode
    }
}

fn bearer(token: &'static str) -> ExpectedRequest {
    let value = format!("Bearer {token}");
    ExpectedRequest::new().header(
        AUTHORIZATION,
        HeaderValue::from_str(&value).expect("bearer test header"),
    )
}

/// Queues the challenge that answers the initial token, followed by the
/// response a replay or the next request would consume.
fn queue_challenge_then(
    executor: &DeterministicExecutor,
    next: ExpectedRequest,
    body: &'static [u8],
) {
    executor
        .application()
        .queue_expected_response(
            bearer(INITIAL),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"challenge")),
        )
        .expect("challenge response");
    executor
        .application()
        .queue_expected_response(next, ScriptedResponse::success(Bytes::from_static(body)))
        .expect("next response");
}

fn challenged(
    core: &ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
) -> Request<ProtectedEndpoint> {
    request(
        core.clone(),
        selector,
        ChallengePolicy::Unauthorized,
        PreparedBody::bytes(Bytes::from_static(b"replayable-body")),
    )
}

fn assert_final_challenge(error: &ApiError, executor: &DeterministicExecutor) {
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Rejected));
    assert_eq!(error.http_status(), Some(StatusCode::UNAUTHORIZED));
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    let counts = error.operation_counts().expect("journal counts");
    assert_eq!(counts.attempts_started(), 1);
    assert_eq!(counts.application_handoffs(), 1);
    assert_eq!(counts.authentication_recoveries(), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        executor.application().remaining_scripts().expect("scripts"),
        1,
        "the next response is never consumed by a replay"
    );
}

#[tokio::test]
async fn retry_02_recovery_mode_none_makes_the_challenge_final_and_keeps_material() {
    let executor = DeterministicExecutor::new();
    queue_challenge_then(&executor, bearer(INITIAL), b"reused");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        ModeProvider::new(RecoveryMode::None, false, &acquires, &refreshes),
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let error = api_error(challenged(&core, &selector).await);
    assert_final_challenge(&error, &executor);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    // `None` neither refreshes nor invalidates: the applied version stays
    // installed, and the next request reuses exactly that material without
    // provider work.
    let installed = selector
        .installed_version()
        .expect("material stays installed");
    assert!(selector.is_set());
    assert_eq!(
        challenged(&core, &selector)
            .await
            .expect("the next request reuses the applied material"),
        Bytes::from_static(b"reused")
    );
    assert_eq!(selector.installed_version(), Some(installed));
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn retry_02_invalidate_only_rejects_exactly_the_applied_version_without_replay() {
    let executor = DeterministicExecutor::new();
    queue_challenge_then(&executor, bearer(NEWER), b"fresh");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        ModeProvider::new(RecoveryMode::InvalidateOnly, false, &acquires, &refreshes),
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let error = api_error(challenged(&core, &selector).await);
    assert_final_challenge(&error, &executor);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    // The applied version is rejected and removed; nothing replaced it.
    assert_eq!(selector.installed_version(), None);
    assert!(!selector.is_set());

    // The next request performs a fresh acquisition (never a refresh of the
    // rejected material) and applies only the newly installed version.
    assert_eq!(
        challenged(&core, &selector)
            .await
            .expect("request with freshly acquired material"),
        Bytes::from_static(b"fresh")
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert!(selector.is_set());
}

#[tokio::test]
async fn retry_02_failing_refresh_during_recovery_never_replays() {
    let executor = DeterministicExecutor::new();
    queue_challenge_then(&executor, ExpectedRequest::new(), b"replay-must-not-happen");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        ModeProvider::new(RecoveryMode::Refresh, true, &acquires, &refreshes),
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));

    let error = api_error(challenged(&core, &selector).await);
    // The authorized refresh ran once and failed: its failure is the result,
    // and no replay was handed to the transport.
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        executor.application().remaining_scripts().expect("scripts"),
        1,
        "the replay response is never consumed"
    );
    // The challenge was observed; the recovery attempt began but was never
    // dispatched.
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::UNAUTHORIZED));
    let last = error.last_attempt().expect("recovery attempt facts");
    assert!(last.attempt().authentication_recovery);
    assert_eq!(last.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    let counts = error.operation_counts().expect("journal counts");
    assert_eq!(counts.application_handoffs(), 1);
    assert_eq!(counts.authentication_recoveries(), 1);
    // EXEC-01: the failed refresh is the linked provider acquisition.
    let provider = error
        .provider_acquisition()
        .expect("failed refresh journal");
    assert_eq!(provider.reason(), CredentialReason::Rejected);
    assert_eq!(provider.outcome(), ProviderAcquisitionOutcome::Failed);
    assert_eq!(provider.parent_operation_id(), error.operation_id());
    assert_eq!(hooks.response.lock().expect("response records").len(), 1);
}
