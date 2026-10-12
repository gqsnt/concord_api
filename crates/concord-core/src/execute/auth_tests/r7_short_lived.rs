use super::*;

struct ShortProvider {
    lifetime: Duration,
    calls: Arc<AtomicUsize>,
    gate: Option<Gate>,
}

impl CredentialProvider for ShortProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(gate) = &self.gate {
                gate.wait().await;
            }
            Ok(short_token(self.lifetime, INITIAL))
        })
    }
}

fn short_token(lifetime: Duration, value: &str) -> AccessToken {
    AccessToken::try_with_expiration(value, Instant::now() + lifetime).expect("expiring token")
}

struct R7ExpiryMaterial {
    expires_at: Instant,
    expired: Arc<AtomicBool>,
    secret: SecretString,
}

impl CredentialMaterial for R7ExpiryMaterial {
    fn expires_at(&self) -> Option<Instant> {
        Some(if self.expired.load(Ordering::SeqCst) {
            Instant::now() - Duration::from_secs(1)
        } else {
            self.expires_at
        })
    }
}

impl SecretCredential for R7ExpiryMaterial {
    fn expose_secret(&self) -> &str {
        self.secret.expose_secret()
    }
}

struct R7ExpiryLimiter {
    expired: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

impl RateLimiter for R7ExpiryLimiter {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "r7.expiry",
            1,
        )
    }
    fn acquire<'a>(
        &'a self,
        _: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.expired.store(true, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
    fn observe(&self, _: RateLimitResponse<'_>) {}
}

#[tokio::test]
async fn r7_short_checked_material_is_rechecked_for_actual_expiry_after_admission() {
    let executor = DeterministicExecutor::new();
    let expired = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::static_value(
        R7ExpiryMaterial {
            expires_at: Instant::now() + Duration::from_secs(30),
            expired: expired.clone(),
            secret: secret_string(INITIAL),
        },
        Duration::ZERO,
    );
    let admissions = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(R7ExpiryLimiter {
            expired,
            calls: admissions.clone(),
        })),
    );
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("r7.actual_expiry"), Method::GET)
            .response::<Binary, Bytes>()
            .build()
            .unwrap();
    let error = bind_protected(
        &core,
        [Auth::bearer(CredentialRef::new(selector))],
        endpoint,
    )
    .await
    .unwrap_err();
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Unavailable));
    assert_eq!(admissions.load(Ordering::SeqCst), 1);
    assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    assert_eq!(executor.application().request_count().unwrap(), 0);
}

fn queue_success(executor: &DeterministicExecutor, value: &'static str) {
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {value}")).expect("test header"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("response queued");
}

async fn r7_assert_provider_lifetimes(lifetimes: &[u64]) {
    for &seconds in lifetimes {
        let executor = DeterministicExecutor::new();
        queue_success(&executor, INITIAL);
        let calls = Arc::new(AtomicUsize::new(0));
        let selector = SlotSelector::provider(
            ShortProvider {
                lifetime: Duration::from_secs(seconds),
                calls: calls.clone(),
                gate: None,
            },
            Duration::ZERO,
        );
        let admissions = Arc::new(AtomicUsize::new(0));
        let (core, _) = core_with_runtime(
            &executor,
            Arc::new(AuthHooks::default()),
            RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::counting(
                &admissions,
                &Arc::new(AtomicUsize::new(0)),
            ))),
        );
        let result = request(
            core,
            &selector,
            ChallengePolicy::Never,
            PreparedBody::bytes(Bytes::new()),
        )
        .await;
        assert!(
            result.is_ok(),
            "lifetime={seconds}; calls={}; admissions={}; handoffs={}; result={result:?}",
            calls.load(Ordering::SeqCst),
            admissions.load(Ordering::SeqCst),
            executor.application().request_count().unwrap()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(admissions.load(Ordering::SeqCst), 1);
        assert_eq!(executor.application().request_count().unwrap(), 1);
    }
}

#[tokio::test]
async fn r7_short_provider_executes_once_at_30_60_and_120_seconds() {
    r7_assert_provider_lifetimes(&[30, 60, 120]).await;
}

#[tokio::test]
async fn r7_control_provider_above_the_skew_executes_once() {
    r7_assert_provider_lifetimes(&[120]).await;
}

#[tokio::test]
async fn r7_short_endpoint_installed_and_static_material_execute_until_expiry() {
    for seconds in [30, 60, 120] {
        for endpoint in [false, true] {
            let executor = DeterministicExecutor::new();
            queue_success(&executor, INITIAL);
            let token = short_token(Duration::from_secs(seconds), INITIAL);
            let selector = if endpoint {
                let selector = SlotSelector::endpoint(Duration::ZERO);
                let ticket = selector.reserve_store_intent().unwrap();
                assert!(selector.install(ticket, token).unwrap().is_installed());
                selector
            } else {
                SlotSelector::static_value(token, Duration::ZERO)
            };
            assert!(selector.is_set());
            let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
            let result = request(
                core,
                &selector,
                ChallengePolicy::Never,
                PreparedBody::bytes(Bytes::new()),
            )
            .await;
            assert!(
                result.is_ok(),
                "lifetime={seconds}; endpoint={endpoint}; result={result:?}"
            );
            assert_eq!(executor.application().request_count().unwrap(), 1);
        }
    }
}

#[tokio::test]
async fn r7_short_material_replacement_and_expiry_after_quota_wait() {
    for expired in [false, true] {
        let executor = DeterministicExecutor::new();
        if !expired {
            queue_success(&executor, NEWER);
        }
        let selector = SlotSelector::endpoint(Duration::ZERO);
        selector
            .replace(short_token(Duration::from_secs(30), INITIAL))
            .unwrap();
        let gate = Gate::closed();
        let admissions = Arc::new(AtomicUsize::new(0));
        let (core, _) = core_with_runtime(
            &executor,
            Arc::new(AuthHooks::default()),
            RuntimeConfigBuilder::new()
                .rate_limiter(Arc::new(AuthRateLimiter::gated(&admissions, gate.clone()))),
        );
        let pending = selector.clone();
        let task = tokio::spawn(async move {
            request(
                core,
                &pending,
                ChallengePolicy::Never,
                PreparedBody::bytes(Bytes::new()),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(2), gate.wait_until_reached())
            .await
            .expect("preflight reaches admission");
        let replacement = if expired {
            AccessToken::try_with_expiration(NEWER, Instant::now() - Duration::from_secs(1))
                .unwrap()
        } else {
            short_token(Duration::from_secs(30), NEWER)
        };
        selector.replace(replacement).unwrap();
        gate.open();
        let result = task.await.unwrap();
        if expired {
            let error = result.expect_err("expired material cannot dispatch");
            assert_eq!(error.auth_kind(), Some(AuthErrorKind::Unavailable));
            assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
            assert_eq!(admissions.load(Ordering::SeqCst), 1);
        } else {
            assert!(result.is_ok(), "replacement result={result:?}");
            assert_eq!(admissions.load(Ordering::SeqCst), 2);
            assert_eq!(executor.application().request_count().unwrap(), 1);
        }
    }
}

#[tokio::test]
async fn r7_short_provider_followers_retain_the_acquired_version() {
    let executor = DeterministicExecutor::new();
    queue_success(&executor, INITIAL);
    queue_success(&executor, INITIAL);
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Gate::closed();
    let selector = SlotSelector::provider(
        ShortProvider {
            lifetime: Duration::from_secs(30),
            calls: calls.clone(),
            gate: Some(gate.clone()),
        },
        Duration::ZERO,
    );
    let admissions = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::counting(
            &admissions,
            &Arc::new(AtomicUsize::new(0)),
        ))),
    );
    let (a, b) = (selector.clone(), selector.clone());
    let other = core.clone();
    let first = tokio::spawn(async move {
        request(
            core,
            &a,
            ChallengePolicy::Never,
            PreparedBody::bytes(Bytes::new()),
        )
        .await
    });
    gate.wait_until_reached().await;
    let second = tokio::spawn(async move {
        request(
            other,
            &b,
            ChallengePolicy::Never,
            PreparedBody::bytes(Bytes::new()),
        )
        .await
    });
    // Poll the follower through preflight before releasing the leader.
    tokio::task::yield_now().await;
    gate.open();
    assert!(first.await.unwrap().is_ok());
    assert!(second.await.unwrap().is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(admissions.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 2);
}
