use super::*;
use http::HeaderName;

struct R8RecoveryProvider {
    lifetime: Duration,
    refreshes: Arc<AtomicUsize>,
    fail_second: bool,
}

struct R8IndependentProvider(Arc<AtomicUsize>);
impl CredentialProvider for R8IndependentProvider {
    type Material = AccessToken;
    fn acquire<'a>(
        &'a self,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(short_token(Duration::from_secs(30), INITIAL)) })
    }
    fn refresh<'a>(
        &'a self,
        _: &'a AccessToken,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(short_token(Duration::from_secs(120), NEWER)) })
    }
}

impl CredentialProvider for R8RecoveryProvider {
    type Material = AccessToken;
    fn acquire<'a>(
        &'a self,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        Box::pin(async { Ok(short_token(Duration::from_secs(120), INITIAL)) })
    }
    fn refresh<'a>(
        &'a self,
        _: &'a AccessToken,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        let call = self.refreshes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if call > 0 && self.fail_second {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            Ok(short_token(self.lifetime, REFRESHED))
        })
    }
    fn recovery_mode(&self) -> crate::auth::RecoveryMode {
        crate::auth::RecoveryMode::Refresh
    }
}

async fn r8_recovery_case(seconds: u64, fail_second: bool, status: StatusCode, shared: bool) {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {INITIAL}")).unwrap(),
            ),
            ScriptedResponse::status(status, Bytes::new()),
        )
        .unwrap();
    let mut expected = ExpectedRequest::new().header(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {REFRESHED}")).unwrap(),
    );
    if shared {
        expected = expected.header(
            HeaderName::from_static("x-shared"),
            HeaderValue::from_static(REFRESHED),
        );
    }
    expected = expected.header(
        HeaderName::from_static("x-independent"),
        HeaderValue::from_static(NEWER),
    );
    executor
        .application()
        .queue_expected_response(
            expected,
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .unwrap();
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        R8RecoveryProvider {
            lifetime: Duration::from_secs(seconds),
            refreshes: refreshes.clone(),
            fail_second,
        },
        Duration::ZERO,
    );
    let credential = CredentialRef::new(selector);
    let mut auth =
        vec![Auth::bearer(credential.clone()).challenge(ChallengePolicy::UnauthorizedOrForbidden)];
    if shared {
        auth.push(Auth::header(HeaderName::from_static("x-shared"), credential).unwrap());
    }
    let independent_calls = Arc::new(AtomicUsize::new(0));
    auth.push(
        Auth::header(
            HeaderName::from_static("x-independent"),
            CredentialRef::new(SlotSelector::provider(
                R8IndependentProvider(independent_calls.clone()),
                Duration::ZERO,
            )),
        )
        .unwrap(),
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let endpoint = RuntimeEndpoint::builder(EndpointId::from_static("r8.recovery"), Method::POST)
        .effect(crate::endpoint::Effect::Idempotent)
        .response::<Binary, Bytes>()
        .build()
        .unwrap();
    let result = bind_protected(&core, auth, endpoint).await;
    println!(
        "R8 recovery seconds={seconds} fail_second={fail_second} status={status} shared={shared} refreshes={} handoffs={} result={result:?}",
        refreshes.load(Ordering::SeqCst),
        executor.application().request_count().unwrap()
    );
    assert!(result.is_ok());
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(independent_calls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 2);
}

#[tokio::test]
async fn r8_short_recovery_retains_preparation_even_when_second_refresh_would_fail() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        for shared in [false, true] {
            r8_recovery_case(30, true, status, shared).await;
        }
    }
}

#[tokio::test]
async fn r8_short_recovery_avoids_a_successful_but_redundant_second_refresh() {
    r8_recovery_case(30, false, StatusCode::UNAUTHORIZED, false).await;
}

#[tokio::test]
async fn r8_recovery_long_lived_control() {
    r8_recovery_case(120, true, StatusCode::UNAUTHORIZED, true).await;
}

struct R8RaceProvider {
    expired: Arc<AtomicBool>,
    refreshes: Arc<AtomicUsize>,
    active: Option<Gate>,
}
impl CredentialProvider for R8RaceProvider {
    type Material = R7ExpiryMaterial;
    fn acquire<'a>(
        &'a self,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            if self.refreshes.load(Ordering::SeqCst) > 0 {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            Ok(self.material(INITIAL, 120))
        })
    }
    fn refresh<'a>(
        &'a self,
        _: &'a Self::Material,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        let call = self.refreshes.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if call > 0 {
                if let Some(gate) = &self.active {
                    gate.wait().await;
                    return Ok(self.material(NEWER, 120));
                }
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            Ok(self.material(REFRESHED, 30))
        })
    }
    fn recovery_mode(&self) -> crate::auth::RecoveryMode {
        crate::auth::RecoveryMode::Refresh
    }
}
impl R8RaceProvider {
    fn material(&self, value: &str, seconds: u64) -> R7ExpiryMaterial {
        R7ExpiryMaterial {
            expires_at: Instant::now() + Duration::from_secs(seconds),
            expired: self.expired.clone(),
            secret: secret_string(value),
        }
    }
}

fn r8_race_request(
    core: &ClientCore,
    selector: &Arc<SlotSelector<R7ExpiryMaterial>>,
    challenge: ChallengePolicy,
) -> BoundRuntimeRequest<Binary, Bytes> {
    let credential = CredentialRef::new(selector.clone());
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("r8.admission_race"), Method::GET)
            .response::<Binary, Bytes>()
            .build()
            .unwrap();
    bind_protected(
        core,
        [
            Auth::bearer(credential.clone()).challenge(challenge),
            Auth::header(HeaderName::from_static("x-shared"), credential).unwrap(),
        ],
        endpoint,
    )
}

#[tokio::test]
async fn r8_recovered_material_rechecks_expiry_clear_replacement_and_rejection_after_admission() {
    for change in ["expiry", "clear", "replacement", "rejection"] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_response(ScriptedResponse::status(
                StatusCode::UNAUTHORIZED,
                Bytes::new(),
            ))
            .unwrap();
        if change == "replacement" {
            executor
                .application()
                .queue_expected_response(
                    ExpectedRequest::new()
                        .header(
                            AUTHORIZATION,
                            HeaderValue::from_str(&format!("Bearer {NEWER}")).unwrap(),
                        )
                        .header(
                            HeaderName::from_static("x-shared"),
                            HeaderValue::from_static(NEWER),
                        ),
                    ScriptedResponse::success(Bytes::from_static(b"ok")),
                )
                .unwrap();
        }
        let expired = Arc::new(AtomicBool::new(false));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let selector = SlotSelector::provider(
            R8RaceProvider {
                expired: expired.clone(),
                refreshes: refreshes.clone(),
                active: None,
            },
            Duration::ZERO,
        );
        let admission = Gate::closed();
        let (core, _) = core_with_runtime(
            &executor,
            Arc::new(AuthHooks::default()),
            RuntimeConfigBuilder::new().rate_limiter(Arc::new(RetainedReservationLimiter {
                acquisitions: AtomicUsize::new(0),
                later_gate: admission.clone(),
            })),
        );
        let pending = r8_race_request(&core, &selector, ChallengePolicy::Unauthorized);
        let task = tokio::spawn(async move { pending.await });
        tokio::time::timeout(Duration::from_secs(2), admission.wait_until_reached())
            .await
            .unwrap();
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        match change {
            "expiry" => expired.store(true, Ordering::SeqCst),
            "clear" => selector.clear().unwrap(),
            "replacement" => {
                selector
                    .replace(R7ExpiryMaterial {
                        expires_at: Instant::now() + Duration::from_secs(120),
                        expired: expired.clone(),
                        secret: secret_string(NEWER),
                    })
                    .unwrap();
            }
            "rejection" => {
                selector
                    .replace(R7ExpiryMaterial {
                        expires_at: Instant::now() + Duration::from_secs(120),
                        expired: expired.clone(),
                        secret: secret_string(NEWER),
                    })
                    .unwrap();
                let prepared = selector
                    .preflight(crate::http::provider::ProviderHttp::unavailable_for_test())
                    .await
                    .unwrap();
                selector.reject_installed(&prepared, true).unwrap();
            }
            _ => unreachable!(),
        }
        admission.open();
        let result = task.await.unwrap();
        println!(
            "R8 admission change={change} ok={} handoffs={}",
            result.is_ok(),
            executor.application().request_count().unwrap()
        );
        assert_eq!(result.is_ok(), change == "replacement");
        assert_eq!(
            executor.application().request_count().unwrap(),
            if change == "replacement" { 2 } else { 1 }
        );
    }
}

#[tokio::test]
async fn r8_recovery_joins_an_already_running_proactive_refresh() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::new(),
        ))
        .unwrap();
    for _ in 0..2 {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .header(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {NEWER}")).unwrap(),
                    )
                    .header(
                        HeaderName::from_static("x-shared"),
                        HeaderValue::from_static(NEWER),
                    ),
                ScriptedResponse::success(Bytes::new()),
            )
            .unwrap();
    }
    let active = Gate::closed();
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        R8RaceProvider {
            expired: Arc::new(AtomicBool::new(false)),
            refreshes: refreshes.clone(),
            active: Some(active.clone()),
        },
        Duration::ZERO,
    );
    let admission = Gate::closed();
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(RetainedReservationLimiter {
            acquisitions: AtomicUsize::new(0),
            later_gate: admission.clone(),
        })),
    );
    let first = r8_race_request(&core, &selector, ChallengePolicy::Unauthorized);
    let first = tokio::spawn(async move { first.await });
    tokio::time::timeout(Duration::from_secs(2), admission.wait_until_reached())
        .await
        .unwrap();
    let other = r8_race_request(&core, &selector, ChallengePolicy::Never);
    let other = tokio::spawn(async move { other.await });
    tokio::time::timeout(Duration::from_secs(2), active.wait_until_reached())
        .await
        .unwrap();
    admission.open();
    tokio::task::yield_now().await;
    active.open();
    assert!(first.await.unwrap().is_ok());
    assert!(other.await.unwrap().is_ok());
    assert_eq!(refreshes.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 3);
}

#[tokio::test]
async fn r8_short_recovery_respects_effect_body_and_send_replay_caps() {
    use crate::controls::CallOptions;
    for case in ["effect", "body", "sends", "replays"] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_response(ScriptedResponse::status(
                StatusCode::UNAUTHORIZED,
                Bytes::new(),
            ))
            .unwrap();
        let refreshes = Arc::new(AtomicUsize::new(0));
        let selector = SlotSelector::provider(
            R8RecoveryProvider {
                lifetime: Duration::from_secs(30),
                refreshes: refreshes.clone(),
                fail_second: true,
            },
            Duration::ZERO,
        );
        let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
        let body = if case == "body" {
            PreparedBody::stream(RequestStream::from_bytes(Bytes::from_static(b"one-shot")))
        } else {
            PreparedBody::bytes(Bytes::new())
        };
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("r8.denied_replay"), Method::POST)
                .effect(if case == "effect" {
                    crate::endpoint::Effect::NonIdempotent
                } else {
                    crate::endpoint::Effect::Idempotent
                })
                .body_repr::<PreparedBody, _>(body)
                .response::<Binary, Bytes>()
                .build()
                .unwrap();
        let pending = bind_protected(
            &core,
            [Auth::bearer(CredentialRef::new(selector)).challenge(ChallengePolicy::Unauthorized)],
            endpoint,
        );
        let options = match case {
            "sends" => CallOptions::new().cap_sends(1),
            "replays" => CallOptions::new().cap_replays(0),
            _ => CallOptions::new(),
        };
        let error = pending.with_options(&options).await.unwrap_err();
        assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
        assert_eq!(executor.application().request_count().unwrap(), 1);
    }
}

#[tokio::test]
async fn r8_new_operation_keeps_proactive_policy_after_short_recovery() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::new(),
        ))
        .unwrap();
    queue_success(&executor, REFRESHED);
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        R8RecoveryProvider {
            lifetime: Duration::from_secs(30),
            refreshes: refreshes.clone(),
            fail_second: true,
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    assert!(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Unauthorized,
            PreparedBody::bytes(Bytes::new())
        )
        .await
        .is_ok()
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    let error = request(
        core,
        &selector,
        ChallengePolicy::Never,
        PreparedBody::bytes(Bytes::new()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(refreshes.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 2);
}

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
