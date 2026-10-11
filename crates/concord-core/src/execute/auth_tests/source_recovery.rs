//! Source-aware authentication recovery (AUTH-01, AUTH-02, RETRY-02) through
//! the managed executor: V3-E11, V3-E16 (execution side) and the
//! authentication part of V3-E17.

use crate::__private::GeneratedCredential;
use crate::controls::CallOptions;
use crate::error::{ClientBuildErrorKind, ClientInputKind, DispatchKnowledge};

use super::*;

fn plain(
    core: &ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
) -> Request<ProtectedEndpoint> {
    request(
        core.clone(),
        selector,
        ChallengePolicy::Unauthorized,
        PreparedBody::bytes(Bytes::new()),
    )
}

fn rotating(
    acquires: &Arc<AtomicUsize>,
    refreshes: &Arc<AtomicUsize>,
    exact: &Arc<AtomicBool>,
) -> RotatingProvider {
    RotatingProvider {
        acquires: Arc::clone(acquires),
        refreshes: Arc::clone(refreshes),
        exact_current_seen: Arc::clone(exact),
    }
}

// V3-E11: missing, expired, rejected or unobtainable material fails before
// any application dispatch, for every source kind.
#[tokio::test]
async fn unusable_material_fails_before_application_dispatch_for_every_source() {
    let executor = DeterministicExecutor::new();
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let never_stored = SlotSelector::endpoint(Duration::ZERO);
    let error = api_error(plain(&core, &never_stored).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);

    let expired = SlotSelector::endpoint(Duration::ZERO);
    expired
        .replace(
            AccessToken::try_from_secret_with_expiration(
                secret_string(INITIAL),
                Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("past instant"),
            )
            .expect("expired token"),
        )
        .expect("expired endpoint material");
    let error = api_error(plain(&core, &expired).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationUnavailable);

    let rejected_static = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let applied = rejected_static
        .preflight(core.provider_http(None))
        .await
        .expect("applied static material");
    rejected_static
        .reject_installed(&applied, false)
        .expect("earlier final challenge");
    let error = api_error(plain(&core, &rejected_static).await);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Rejected));

    let failing_provider = SlotSelector::provider(FailingProvider, Duration::ZERO);
    let error = api_error(plain(&core, &failing_provider).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);

    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(executor.provider().request_count().expect("count"), 0);
}

// AUTH-01 / V3-E11: an endpoint-backed challenge is final, rejects exactly
// the applied installed version and never invokes the login endpoint.
#[tokio::test]
async fn endpoint_backed_challenge_is_final_and_never_invokes_its_source() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"session expired"),
        ))
        .expect("challenge");
    let session = SlotSelector::endpoint(Duration::ZERO);
    session
        .replace(access_token(INITIAL))
        .expect("stored session");
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let error = api_error(plain(&core, &session).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(!session.is_set());
    let later = api_error(plain(&core, &session).await);
    assert_eq!(later.auth_kind(), Some(AuthErrorKind::Rejected));
    assert_eq!(later.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    // One challenged application request; no implicit login, no replay.
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        session.counters_for_test(),
        (1, 1),
        "rejection is not a clear"
    );
}

// RETRY-02 / V3-E17 (authentication part): a non-idempotent operation that
// received a challenge is never resubmitted automatically, even with a
// refresh-capable provider; the applied version is rejected so the next
// call refreshes before dispatch.
#[tokio::test]
async fn non_idempotent_challenge_never_replays_and_rejects_the_applied_version() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .header(
                    http::HeaderName::from_static("x-secondary-token"),
                    HeaderValue::from_static(NEWER),
                ),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"challenge")),
        )
        .expect("challenge on a non-idempotent write");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let exact = Arc::new(AtomicBool::new(false));
    let first = SlotSelector::provider(rotating(&acquires, &refreshes, &exact), Duration::ZERO);
    let second = SlotSelector::static_value(access_token(NEWER), Duration::ZERO);
    let body_calls = Arc::new(AtomicUsize::new(0));
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let write = |core: &ClientCore| {
        Request::new(
            core.clone(),
            MultiAuthEndpoint {
                first: CredentialRef::new(Arc::clone(&first)),
                second: CredentialRef::new(Arc::clone(&second)),
                first_challenge: ChallengePolicy::Unauthorized,
                body_calls: Arc::clone(&body_calls),
            },
        )
    };

    let error = api_error(write(&core).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.http_status(), Some(StatusCode::UNAUTHORIZED));
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        refreshes.load(Ordering::SeqCst),
        0,
        "no refresh for a forbidden replay"
    );
    assert_eq!(
        body_calls.load(Ordering::SeqCst),
        1,
        "the body was never rebuilt"
    );
    assert!(!first.is_set(), "the applied provider version is rejected");
    assert!(second.is_set(), "the unattributed placement is untouched");

    // The next call refreshes exactly the rejected version before dispatch.
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"written")),
        )
        .expect("later write with refreshed material");
    assert_eq!(
        write(&core).await.expect("later write"),
        Bytes::from_static(b"written")
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(exact.load(Ordering::SeqCst));
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
}

// RETRY-02: authentication recovery counts toward replay and send caps.
#[tokio::test]
async fn replay_and_send_caps_bound_authentication_recovery() {
    for options in [
        CallOptions::new().cap_replays(0),
        CallOptions::new().cap_sends(1),
    ] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_response(ScriptedResponse::status(
                StatusCode::UNAUTHORIZED,
                Bytes::new(),
            ))
            .expect("challenge");
        let acquires = Arc::new(AtomicUsize::new(0));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let exact = Arc::new(AtomicBool::new(false));
        let selector =
            SlotSelector::provider(rotating(&acquires, &refreshes, &exact), Duration::ZERO);
        let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
        let mut capped = plain(&core, &selector);
        capped.options.apply_call_options(&options);
        let error = api_error(capped.await);
        assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
        assert_eq!(executor.application().request_count().expect("count"), 1);
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    }

    // Positive control: with room for exactly one replay and two sends the
    // same idempotent request recovers once.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::new(),
        ))
        .expect("challenge");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"recovered")),
        )
        .expect("recovered response");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let exact = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(rotating(&acquires, &refreshes, &exact), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let mut bounded = plain(&core, &selector);
    bounded
        .options
        .apply_call_options(&CallOptions::new().cap_replays(1).cap_sends(2));
    assert_eq!(
        bounded.await.expect("one permitted recovery"),
        Bytes::from_static(b"recovered")
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

struct WrongOriginProvider {
    origins: Vec<ProviderOrigin>,
    target: &'static str,
}

impl CredentialProvider for WrongOriginProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            context
                .http
                .send(ProviderRequest::new(
                    Method::POST,
                    url::Url::parse(self.target).expect("provider target"),
                ))
                .await?;
            Ok(access_token(INITIAL))
        })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

// AUTH-02 / V3-E16: a provider trying an undeclared origin (or declaring
// none) is rejected before any provider transport call.
#[tokio::test]
async fn provider_helper_refuses_undeclared_origins_before_transport() {
    for provider in [
        WrongOriginProvider {
            origins: vec![
                ProviderOrigin::parse("https://declared-provider.example.test")
                    .expect("declared origin"),
            ],
            target: "https://undeclared-provider.example.test/token",
        },
        WrongOriginProvider {
            origins: Vec::new(),
            target: "https://declared-provider.example.test/token",
        },
    ] {
        let executor = DeterministicExecutor::new();
        let selector = SlotSelector::provider(provider, Duration::ZERO);
        let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
        let error = api_error(plain(&core, &selector).await);
        assert_eq!(error.auth_kind(), Some(AuthErrorKind::InvalidConfiguration));
        assert!(!error.to_string().contains("undeclared-provider"));
        assert_eq!(executor.provider().request_count().expect("count"), 0);
        assert_eq!(executor.application().request_count().expect("count"), 0);
    }
}

struct HangingProvider {
    gate: Gate,
}

impl CredentialProvider for HangingProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            self.gate.wait().await;
            Ok(access_token(INITIAL))
        })
    }
}

// AUTH-02 / §20.1: one acquisition runs under min(remaining operation
// deadline, provider timeout).
#[tokio::test(start_paused = true)]
async fn provider_acquisition_is_bounded_by_the_configured_provider_timeout() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::provider(
        HangingProvider {
            gate: Gate::closed(),
        },
        Duration::ZERO,
    );
    let (core, _) = core_with_runtime(
        &executor,
        Arc::new(AuthHooks::default()),
        RuntimeConfigBuilder::new().provider_timeout(Duration::from_secs(1)),
    );
    let started = tokio::time::Instant::now();
    let error = api_error(plain(&core, &selector).await);
    let elapsed = started.elapsed();
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert!(elapsed >= Duration::from_secs(1));
    assert!(
        elapsed < Duration::from_secs(30),
        "the provider ceiling won"
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

struct DeclaringProvider {
    origins: Vec<ProviderOrigin>,
}

impl DeclaringProvider {
    fn with(count: usize, scheme: &str) -> Self {
        Self {
            origins: (0..count)
                .map(|index| {
                    ProviderOrigin::parse(format!("{scheme}://p{index}.provider.example.test"))
                        .expect("valid origin")
                })
                .collect(),
        }
    }
}

impl CredentialProvider for DeclaringProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async { Err(AuthError::new(AuthErrorKind::Unavailable)) })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

fn build_core(executor: &DeterministicExecutor, runtime: RuntimeConfigBuilder) -> ClientCore {
    ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        runtime,
        executor,
    )
    .expect("development core")
}

fn origin_build_error(
    result: Result<GeneratedCredential<AccessToken>, crate::error::ClientBuildError>,
) -> crate::error::ClientBuildError {
    match result {
        Ok(_) => panic!("declared origin set must be rejected at build"),
        Err(error) => error,
    }
}

// AUTH-02 / V3-E16: declared origins are validated at client build (8 by
// default, configurable up to 32, HTTPS only) and replacement cannot widen
// them past the capacity.
#[tokio::test]
async fn declared_provider_origins_are_validated_at_build_and_on_replacement() {
    let executor = DeterministicExecutor::new();
    let default_core = build_core(&executor, RuntimeConfigBuilder::new());

    GeneratedCredential::provider(
        &default_core,
        "session_provider",
        DeclaringProvider::with(8, "https"),
    )
    .expect("eight origins fit the default capacity");
    let error = origin_build_error(GeneratedCredential::provider(
        &default_core,
        "session_provider",
        DeclaringProvider::with(9, "https"),
    ));
    assert_eq!(error.kind(), ClientBuildErrorKind::InvalidProviderOrigins);
    assert_eq!(error.input_field(), Some("session_provider"));
    assert_eq!(
        error.input_kind(),
        Some(ClientInputKind::CredentialProvider)
    );
    assert!(!error.to_string().contains("provider.example.test"));

    let cleartext = origin_build_error(GeneratedCredential::provider(
        &default_core,
        "session_provider",
        DeclaringProvider::with(1, "http"),
    ));
    assert_eq!(
        cleartext.kind(),
        ClientBuildErrorKind::InvalidProviderOrigins
    );

    let shared: Arc<dyn CredentialProvider<Material = AccessToken>> =
        Arc::new(DeclaringProvider::with(9, "https"));
    let error = origin_build_error(GeneratedCredential::provider_arc(
        &default_core,
        "shared_provider",
        shared,
    ));
    assert_eq!(error.input_field(), Some("shared_provider"));

    let wide_core = build_core(
        &executor,
        RuntimeConfigBuilder::new().provider_max_origins(crate::limits::MAX_PROVIDER_ORIGINS),
    );
    let credential = GeneratedCredential::provider(
        &wide_core,
        "session_provider",
        DeclaringProvider::with(crate::limits::MAX_PROVIDER_ORIGINS, "https"),
    )
    .expect("the hard maximum fits a configured capacity of 32");
    let error = credential
        .replace_provider(DeclaringProvider::with(
            crate::limits::MAX_PROVIDER_ORIGINS + 1,
            "https",
        ))
        .expect_err("replacement cannot exceed the capacity");
    assert_eq!(error.kind(), AuthErrorKind::InvalidConfiguration);
    credential
        .replace_provider(DeclaringProvider::with(2, "https"))
        .expect("replacement within the capacity");

    let invalid_capacity = ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().provider_max_origins(33),
        &executor,
    );
    let error = match invalid_capacity {
        Ok(_) => panic!("capacity above the hard maximum is invalid"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ClientBuildErrorKind::RuntimeConfiguration);
    assert_eq!(error.input_field(), Some("provider_max_origins"));
}

// Synchronous generated credential controls act on the shared store without
// network work (§23.1 clear_session / replace_session bridge).
#[tokio::test]
async fn generated_credential_controls_are_synchronous_local_and_shared() {
    let executor = DeterministicExecutor::new();
    let core = build_core(&executor, RuntimeConfigBuilder::new());
    let session = GeneratedCredential::<AccessToken>::endpoint(&core);
    let clone = session.clone();
    assert_eq!(session.installed_version(), None);
    let version = session
        .replace(access_token(INITIAL))
        .expect("replace_session");
    assert_eq!(clone.installed_version(), Some(version));
    assert!(clone.is_set());
    let error = session
        .replace_result(AccessToken::try_new(
            "x".repeat(MAX_CREDENTIAL_SECRET_BYTES + 1),
        ))
        .expect_err("invalid built-in material");
    assert_eq!(error.kind(), AuthErrorKind::MaterialTooLarge);
    assert_eq!(
        clone.installed_version(),
        Some(version),
        "failure keeps material"
    );
    clone.clear().expect("clear_session");
    assert!(!session.is_set());
    session
        .clear()
        .expect("clearing an empty store still succeeds");
    assert_eq!(executor.application().request_count().expect("count"), 0);
}
