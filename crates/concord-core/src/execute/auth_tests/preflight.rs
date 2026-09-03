use super::*;

#[tokio::test]
async fn configured_key_limit_failure_cuts_off_every_protected_side_effect_and_redacts_key() {
    const KEY_SENTINEL: &str = "configured-key-private-73cf9a0d";
    const KEY_LIMIT: usize = 64;
    const KEY_PART_NAME_BYTES: usize = "client.tenant".len();
    let executor = DeterministicExecutor::new();
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&provider_calls),
        },
        Duration::ZERO,
    );
    let body_calls = Arc::new(AtomicUsize::new(0));
    let hooks = Arc::new(AuthHooks::default());
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let observations = Arc::new(AtomicUsize::new(0));
    let mut selected_limiter = AuthRateLimiter::counting(&acquisitions, &observations);
    selected_limiter.key_limit = KEY_LIMIT;
    let limiter: Arc<dyn RateLimiter> = Arc::new(selected_limiter);
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let mut rate_key = KEY_SENTINEL.to_owned();
    rate_key.extend(std::iter::repeat_n(
        'x',
        KEY_LIMIT + 1 - KEY_PART_NAME_BYTES - rate_key.len(),
    ));
    assert_eq!(rate_key.len() + KEY_PART_NAME_BYTES, KEY_LIMIT + 1);
    let error = api_error(
        Request::new(
            core,
            ProtectedEndpoint {
                credential: CredentialRef::new(selector),
                challenge: ChallengePolicy::Never,
                body: RequestBody::factory(ReplayFactory {
                    calls: Arc::clone(&body_calls),
                }),
                query: false,
                rate_key: Some(rate_key),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);
    assert_eq!(acquisitions.load(Ordering::SeqCst), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert!(hooks.response.lock().expect("response records").is_empty());
    assert_eq!(
        hooks.completion.lock().expect("completion records").len(),
        1
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(KEY_SENTINEL));
        assert!(!format!("{current:?}").contains(KEY_SENTINEL));
        source = current.source();
    }
    let debug = debug.lock().expect("debug output");
    assert_eq!(debug.len(), 1);
    assert!(debug[0].starts_with("completion "));
    assert!(!debug[0].contains(KEY_SENTINEL));
}

#[tokio::test]
async fn body_failure_prevents_provider_debug_and_native_send() {
    let executor = DeterministicExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &Arc::new(AtomicUsize::new(0)),
    ));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::factory(FailingFactory),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let debug = debug.lock().expect("debug output");
    assert_eq!(debug.len(), 1);
    assert!(debug[0].starts_with("completion "));
    assert_eq!(hooks.before.lock().expect("before records").len(), 1);
    assert!(hooks.response.lock().expect("response records").is_empty());
}

#[tokio::test]
async fn complete_plan_preflight_failure_preserves_earlier_acquisition_and_stops_send() {
    let executor = DeterministicExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let first = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let second = SlotSelector::endpoint(Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &Arc::new(AtomicUsize::new(0)),
    ));
    let (core, debug) = core_with_runtime(
        &executor,
        hooks,
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let body_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = MultiAuthEndpoint {
        first: CredentialRef::new(Arc::clone(&first)),
        second: CredentialRef::new(second),
        first_challenge: ChallengePolicy::Never,
        body_calls: Arc::clone(&body_calls),
    };
    let error = api_error(Request::new(core, endpoint).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(first.is_set());
    assert_eq!(body_calls.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[tokio::test]
async fn limiter_failure_is_typed_and_prevents_finalization_debug_and_send() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(Arc::new(AuthRateLimiter::failing(&limiter_acquires))),
    );
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(crate::error::RateLimitErrorKind::Acquire)
    );
    assert!(
        error
            .to_string()
            .contains("safe application limiter rejection")
    );
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(hooks.response.lock().expect("responses").is_empty());
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[tokio::test]
async fn limiter_rejection_precedes_custom_authorization_materialization() {
    let executor = DeterministicExecutor::new();
    let exposures = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::static_value(
        CountedExposureToken {
            value: secret_string("unexposed-custom-authorization-6fb2"),
            exposures: Arc::clone(&exposures),
        },
        Duration::ZERO,
    );
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new()
            .rate_limiter(Arc::new(AuthRateLimiter::failing(&limiter_acquires))),
    );
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::from_static("runtime.authorization_limiter_order"),
        Method::GET,
    )
    .auth(
        Auth::authorization_with_scheme("Token", CredentialRef::new(selector))
            .expect("valid static Authorization scheme"),
    )
    .accepted_bytes()
    .build()
    .expect("custom Authorization endpoint");

    let error = Request::new(core, endpoint)
        .await
        .expect_err("limiter rejection");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(exposures.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.contains("unexposed-custom-authorization-6fb2"))
    );
}

#[tokio::test]
async fn clear_during_limiter_wait_prevents_stale_credential_send() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let pending_selector = Arc::clone(&selector);
    let task = tokio::spawn(async move {
        request(
            core,
            &pending_selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await
    });
    gate.wait_until_reached().await;
    selector.clear().expect("clear while limiter waits");
    gate.open();
    let error = api_error(task.await.expect("request task"));
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn complete_source_replacement_during_wait_uses_only_the_new_slot() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer newer-auth-token-265f0a"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"new slot")),
        )
        .expect("new-slot response");
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let pending_selector = Arc::clone(&selector);
    let task = tokio::spawn(async move {
        request(
            core,
            &pending_selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await
    });
    gate.wait_until_reached().await;
    selector
        .replace_static(access_token(NEWER))
        .expect("complete source replacement");
    gate.open();
    assert_eq!(
        task.await.expect("request task").expect("new-slot send"),
        Bytes::from_static(b"new slot")
    );
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn repeated_placements_reuse_one_attempt_local_credential_result() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .header(
                    http::HeaderName::from_static("x-secondary-token"),
                    HeaderValue::from_static(INITIAL),
                ),
            ScriptedResponse::success(Bytes::from_static(b"shared")),
        )
        .expect("shared credential response");
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let body_calls = Arc::new(AtomicUsize::new(0));
    let output = Request::new(
        core,
        MultiAuthEndpoint {
            first: CredentialRef::new(Arc::clone(&selector)),
            second: CredentialRef::new(Arc::clone(&selector)),
            first_challenge: ChallengePolicy::Never,
            body_calls: Arc::clone(&body_calls),
        },
    )
    .await
    .expect("shared credential request");
    assert_eq!(output, Bytes::from_static(b"shared"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(body_calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn material_public_collision_fails_preparation_before_body_provider_or_send() {
    let executor = DeterministicExecutor::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, Arc::clone(&hooks));
    let body_calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        Request::new(
            core,
            DynamicCollisionEndpoint {
                credential: CredentialRef::new(selector),
                body_calls: Arc::clone(&body_calls),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(debug.lock().expect("debug output").len(), 1);
}

#[tokio::test]
async fn finalization_expiration_prevents_native_construction_debug_and_send() {
    let executor = DeterministicExecutor::new();
    let checks = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::static_value(
        FinalizationExpiryToken {
            value: secret_string(INITIAL),
            freshness_checks: Arc::clone(&checks),
            expire_after_preflight: true,
        },
        Duration::ZERO,
    );
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let task = tokio::spawn(async move {
        Request::new(
            core,
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(selector),
            },
        )
        .await
    });
    gate.wait_until_reached().await;
    gate.open();
    let error = api_error(task.await.expect("request task"));
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert!(checks.load(Ordering::SeqCst) >= 2);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(
        debug
            .lock()
            .expect("debug output")
            .iter()
            .all(|line| !line.starts_with("request-ready "))
    );
}

#[tokio::test]
async fn reservation_is_retained_after_credential_finalization_failure() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer newer-auth-token-265f0a"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"after finalization failure")),
        )
        .expect("post-failure response");
    let later_gate = Gate::closed();
    let limiter = Arc::new(RetainedReservationLimiter {
        acquisitions: AtomicUsize::new(0),
        later_gate: later_gate.clone(),
    });
    let selected: Arc<dyn RateLimiter> = limiter.clone();
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(selected),
    );
    let expiring = SlotSelector::static_value(
        FinalizationExpiryToken {
            value: secret_string(INITIAL),
            freshness_checks: Arc::new(AtomicUsize::new(0)),
            expire_after_preflight: true,
        },
        Duration::ZERO,
    );
    let error = api_error(
        Request::new(
            core.clone(),
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(expiring),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let stable = SlotSelector::static_value(
        FinalizationExpiryToken {
            value: secret_string(NEWER),
            freshness_checks: Arc::new(AtomicUsize::new(0)),
            expire_after_preflight: false,
        },
        Duration::ZERO,
    );
    let later = tokio::spawn(async move {
        Request::new(
            core,
            FinalizationExpiryEndpoint {
                credential: CredentialRef::new(stable),
            },
        )
        .await
    });
    later_gate.wait_until_reached().await;
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 2);
    later_gate.open();
    assert_eq!(
        later.await.expect("later task").expect("later send"),
        Bytes::from_static(b"after finalization failure")
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn expiry_during_limiter_wait_refreshes_at_current_finalization() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"refreshed after wait")),
        )
        .expect("refreshed response");
    let expired = Arc::new(AtomicBool::new(false));
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        LimiterWaitExpiryProvider {
            initial_expired: Arc::clone(&expired),
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
        },
        Duration::ZERO,
    );
    let gate = Gate::closed();
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::gated(
            &limiter_acquires,
            gate.clone(),
        ))),
    );
    let task = tokio::spawn(async move {
        Request::new(
            core,
            LimiterWaitExpiryEndpoint {
                credential: CredentialRef::new(selector),
            },
        )
        .await
    });
    gate.wait_until_reached().await;
    expired.store(true, Ordering::SeqCst);
    gate.open();
    assert_eq!(
        task.await.expect("request task").expect("refreshed send"),
        Bytes::from_static(b"refreshed after wait")
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn invalid_native_auth_value_fails_after_safe_debug_and_before_execution() {
    const INVALID: &str = "invalid-auth-sentinel-91af\r\nnot-a-header";
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::static_value(access_token(INVALID), Duration::ZERO);
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let rendered = debug.lock().expect("debug output").join("\n");
    assert!(rendered.contains("request"));
    assert!(!rendered.contains("invalid-auth-sentinel-91af"));
}

#[tokio::test]
async fn custom_provider_oversized_material_is_revalidated_at_the_core_boundary() {
    const SENTINEL: &str = "provider-material-sentinel-82dc";
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, Arc::clone(&hooks));
    let selector = SlotSelector::provider(OversizedMaterialProvider, Duration::ZERO);

    let error = api_error(
        Request::new(
            core,
            OversizedMaterialEndpoint {
                credential: CredentialRef::new(selector),
            },
        )
        .await,
    );

    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::MaterialTooLarge));
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.before.lock().expect("before records").len(), 1);
    let diagnostics = format!(
        "{error:?}\n{}",
        debug.lock().expect("debug output").join("\n")
    );
    assert!(!diagnostics.contains(SENTINEL));
}

#[tokio::test]
async fn authenticated_cleartext_fails_in_public_preparation_before_hook_body_or_provider() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(AuthHooks::default());
    let core = ClientCore::build_with_development(
        "http://cleartext.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .request_gate(hooks.clone())
            .response_gate(hooks.clone())
            .completion_observer(hooks.clone()),
        &executor,
    )
    .expect("cleartext core remains valid for public endpoints");
    let calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        CountingProvider {
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let body_calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::factory(ReplayFactory {
                calls: Arc::clone(&body_calls),
            }),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidAuthentication)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(body_calls.load(Ordering::SeqCst), 0);
    assert!(hooks.before.lock().expect("before records").is_empty());
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn request_debug_is_emitted_before_native_boundary_and_never_contains_auth() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(gate.clone())
        .expect("pause application boundary");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("response");
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let exposed_after_debug = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::static_value(
        ExposureToken {
            value: secret_string(INITIAL),
            debug: Arc::clone(&debug),
            exposed_after_debug: Arc::clone(&exposed_after_debug),
        },
        Duration::ZERO,
    );
    let pending = Request::new(
        core,
        ExposureEndpoint {
            credential: CredentialRef::new(selector),
        },
    );
    let task = tokio::spawn(async move { pending.await });
    gate.wait_until_reached().await;
    assert!(exposed_after_debug.load(Ordering::SeqCst));
    let rendered = debug.lock().expect("debug output").join("\n");
    assert!(rendered.contains("request"));
    assert!(!rendered.contains(INITIAL));
    gate.open();
    assert_eq!(
        task.await.expect("request task").expect("request result"),
        Bytes::from_static(b"ok")
    );
}
