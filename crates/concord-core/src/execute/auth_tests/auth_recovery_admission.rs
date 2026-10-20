use super::*;
use crate::controls::CancellationToken;
use crate::error::{DispatchKnowledge, FailureStage, ProviderAcquisitionOutcome};

struct ShortRecoveryProvider(Arc<AtomicUsize>);
impl CredentialProvider for ShortRecoveryProvider {
    type Material = AccessToken;
    fn acquire<'a>(
        &'a self,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        Box::pin(async {
            Ok(
                AccessToken::try_with_expiration(
                    INITIAL,
                    Instant::now() + Duration::from_secs(120),
                )
                .unwrap(),
            )
        })
    }
    fn refresh<'a>(
        &'a self,
        current: &'a AccessToken,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        assert_eq!(current.expose_secret(), INITIAL);
        assert_eq!(context.reason, CredentialReason::Rejected);
        assert_eq!(
            self.0.fetch_add(1, Ordering::SeqCst),
            0,
            "no redundant refresh"
        );
        Box::pin(async {
            Ok(AccessToken::try_with_expiration(
                REFRESHED,
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap())
        })
    }
}
struct AdmissionGates {
    calls: AtomicUsize,
    committed: AtomicUsize,
    recovered: Gate,
    restarted: Gate,
}
impl RateLimiter for AdmissionGates {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "s1.composed",
            1,
        )
    }
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        assert!(!request.plan.is_empty(), "real effective quota declaration");
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            match index {
                0 => (),
                1 => self.recovered.wait().await,
                2 => self.restarted.wait().await,
                _ => panic!("unexpected extra admission"),
            }
            self.committed.fetch_add(1, Ordering::SeqCst);
            Ok(RateLimitPermit::granted())
        })
    }
    fn observe(&self, _: RateLimitResponse<'_>) {}
}

#[tokio::test]
async fn s1_short_recovery_quota_mutation_restart_then_cancel_with_control() {
    for cancel in [false, true] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new().header(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {INITIAL}")).unwrap(),
                ),
                ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::new()),
            )
            .unwrap();
        if !cancel {
            executor
                .application()
                .queue_expected_response(
                    ExpectedRequest::new().header(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {NEWER}")).unwrap(),
                    ),
                    ScriptedResponse::success(Bytes::from_static(b"ok")),
                )
                .unwrap();
        }
        let refreshes = Arc::new(AtomicUsize::new(0));
        let selector =
            SlotSelector::provider(ShortRecoveryProvider(refreshes.clone()), Duration::ZERO);
        let limiter = Arc::new(AdmissionGates {
            calls: AtomicUsize::new(0),
            committed: AtomicUsize::new(0),
            recovered: Gate::closed(),
            restarted: Gate::closed(),
        });
        let hooks = Arc::new(AuthHooks::default());
        let (core, _) = core_with_runtime(
            &executor,
            hooks.clone(),
            RuntimeConfigBuilder::new().rate_limiter(limiter.clone()),
        );
        let body_creates = Arc::new(AtomicUsize::new(0));
        let token = CancellationToken::new();
        let pending = Request::new(
            core,
            ProtectedEndpoint {
                credential: CredentialRef::new(selector.clone()),
                challenge: ChallengePolicy::Unauthorized,
                body: PreparedBody::factory(ReplayFactory {
                    calls: body_creates.clone(),
                }),
                query: false,
                rate_key: Some("protected-quota-key".into()),
            },
        )
        .cancel_with(token.clone())
        .unwrap();
        let task = tokio::spawn(async move { pending.response().await });
        tokio::time::timeout(
            Duration::from_secs(2),
            limiter.recovered.wait_until_reached(),
        )
        .await
        .unwrap();
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        let short_version = selector.installed_version().unwrap();
        let newer_version = selector
            .replace(
                AccessToken::try_with_expiration(NEWER, Instant::now() + Duration::from_secs(120))
                    .unwrap(),
            )
            .unwrap();
        assert_ne!(short_version, newer_version);
        limiter.recovered.open();
        tokio::time::timeout(
            Duration::from_secs(2),
            limiter.restarted.wait_until_reached(),
        )
        .await
        .unwrap();
        assert_eq!(limiter.committed.load(Ordering::SeqCst), 2);
        if cancel {
            token.cancel();
        } else {
            limiter.restarted.open();
        }
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        if cancel {
            let error = api_error(result);
            assert_eq!(error.category(), ErrorCategory::Cancelled);
            assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::ResponseObserved
            );
            assert_eq!(error.last_response_status(), Some(StatusCode::UNAUTHORIZED));
            let counts = error.operation_counts().unwrap();
            assert_eq!(counts.quota_admissions(), 2);
            assert_eq!(counts.freshness_restarts(), 1);
            assert_eq!(counts.application_handoffs(), 1);
            assert_eq!(counts.attempts_started(), 2);
            assert_eq!(counts.authentication_recoveries(), 1);
            assert_eq!(counts.policy_retries(), 0);
            assert_eq!(
                error.last_attempt().unwrap().dispatch_knowledge(),
                DispatchKnowledge::NotDispatched
            );
            let provider = error.provider_acquisition().unwrap();
            assert_eq!(provider.reason(), CredentialReason::Rejected);
            assert_eq!(provider.outcome(), ProviderAcquisitionOutcome::Installed);
            assert_eq!(provider.parent_operation_id(), error.operation_id());
            println!(
                "S1_COMPOSE cancel=true committed=2 restart=1 attempts=2 recoveries=1 app_handoffs=1 status=401 aggregate=ResponseObserved last=NotDispatched provider=Installed"
            );
        } else {
            let response = result.expect("replacement positive control");
            assert_eq!(response.value(), &Bytes::from_static(b"ok"));
            assert_eq!(response.metadata().attempts_started(), 2);
            assert!(response.metadata().recovered_authentication());
            assert_eq!(limiter.committed.load(Ordering::SeqCst), 3);
            assert_eq!(executor.application().request_count().unwrap(), 2);
            println!(
                "S1_COMPOSE cancel=false committed=3 restart=1 attempts=2 recoveries=1 app_handoffs=2 exact_newer_material=true"
            );
        }
        assert_eq!(limiter.calls.load(Ordering::SeqCst), 3);
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        assert_eq!(body_creates.load(Ordering::SeqCst), 2);
        assert_eq!(selector.installed_version(), Some(newer_version));
        assert_eq!(hooks.completion.lock().unwrap().len(), 1);
        assert_eq!(executor.application().remaining_scripts().unwrap(), 0);
    }
}
