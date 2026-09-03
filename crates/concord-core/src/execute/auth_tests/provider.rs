use super::*;

#[tokio::test]
async fn real_tls_provider_http_remains_outside_application_limiter_lifecycle() {
    let provider_server = TlsServer::start().expect("TLS provider loopback server");
    let application_server = TlsServer::start().expect("TLS application loopback server");
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        LoopbackHttpProvider {
            url: url::Url::parse(&provider_server.url("localhost", "/token"))
                .expect("provider URL"),
            calls: Arc::clone(&provider_calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let core = ClientCore::build(
        &application_server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(application_server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .request_gate(hooks.clone())
            .response_gate(hooks.clone())
            .completion_observer(hooks.clone())
            .rate_limiter(limiter),
    )
    .expect("managed application/provider core");

    assert_eq!(
        request(
            core,
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await
        .expect("provider-backed real application request"),
        Bytes::from_static(b"ok")
    );
    provider_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("provider request reached TLS loopback");
    application_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("application request reached TLS loopback");
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider_server.requests().len(), 1);
    assert_eq!(
        provider_server.requests()[0].header("x-provider-probe"),
        Some("isolated-provider-probe")
    );
    assert_eq!(application_server.requests().len(), 1);
    assert_eq!(
        application_server.requests()[0].header("authorization"),
        Some("Bearer real-provider-application-token")
    );
    assert_eq!(hooks.before.lock().expect("before hooks").len(), 1);
    assert_eq!(hooks.response.lock().expect("response hooks").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion hooks").len(), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn paginated_provider_http_never_enters_page_lifecycle_or_limiter_work() {
    let provider_server = TlsServer::start().expect("TLS pagination provider server");
    let application_server = TlsServer::start().expect("TLS pagination application server");
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        LoopbackHttpProvider {
            url: url::Url::parse(&provider_server.url("localhost", "/token"))
                .expect("provider URL"),
            calls: Arc::clone(&provider_calls),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let core = ClientCore::build(
        &application_server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(application_server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .request_gate(hooks.clone())
            .response_gate(hooks.clone())
            .completion_observer(hooks.clone())
            .rate_limiter(limiter),
    )
    .expect("managed pagination application/provider core");

    let items = PaginatedRequest::new(
        Request::new(
            core,
            PaginatedProtectedEndpoint {
                credential: CredentialRef::new(Arc::clone(&selector)),
                page: 0,
            },
        ),
        PageLimit::Prefix(1),
    )
    .await
    .expect("provider-backed pagination");
    assert_eq!(items, b"ok");
    provider_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("provider request reached TLS loopback");
    application_server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("application page reached TLS loopback");

    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider_server.requests().len(), 1);
    assert_eq!(application_server.requests().len(), 1);
    assert_eq!(hooks.before.lock().expect("before hooks").len(), 1);
    assert_eq!(hooks.response.lock().expect("response hooks").len(), 1);
    assert!(
        hooks
            .completion
            .lock()
            .expect("completion hooks")
            .is_empty()
    );
    assert_eq!(hooks.pagination.lock().expect("pagination hooks").len(), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn provider_http_data_stays_outside_application_hooks_debug_and_channel() {
    const PROVIDER_HEADER: &str = "provider-header-5c2ed86e9c70";
    const PROVIDER_REQUEST: &str = "provider-request-body-723ad0d87fc1";
    const PROVIDER_RESPONSE: &str = "provider-response-body-f7ae16b96f2d";
    const APPLICATION_TOKEN: &str = "application-token-e6cc09dfdf24";
    let executor = DeterministicExecutor::new();
    executor
        .provider()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://isolated-provider.example.test/token")
                .header(
                    http::HeaderName::from_static("x-provider-secret"),
                    HeaderValue::from_static(PROVIDER_HEADER),
                )
                .body(Bytes::from_static(PROVIDER_REQUEST.as_bytes())),
            ScriptedResponse::success(Bytes::from_static(PROVIDER_RESPONSE.as_bytes())),
        )
        .expect("provider response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer application-token-e6cc09dfdf24"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"application")),
        )
        .expect("application response");
    let selector = SlotSelector::provider(IsolatedHttpProvider, Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(Arc::new(AuthRateLimiter::counting(
            &limiter_acquires,
            &limiter_observations,
        ))),
    );
    let result = request(
        core,
        &selector,
        ChallengePolicy::Never,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("provider-backed application request");
    assert_eq!(result, Bytes::from_static(b"application"));
    assert_eq!(hooks.before.lock().expect("before hooks").len(), 1);
    assert_eq!(hooks.response.lock().expect("response hooks").len(), 1);
    assert_eq!(hooks.completion.lock().expect("completion hooks").len(), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 1);

    let provider_capture = executor.provider().captures().expect("provider captures");
    assert_eq!(provider_capture.len(), 1);
    assert!(provider_capture[0].observed_url().is_none());
    assert!(provider_capture[0].headers().iter().any(|header| {
        header.name() == "x-provider-secret" && header.value() == &CapturedHeaderValue::Redacted
    }));
    let application_capture = executor
        .application()
        .captures()
        .expect("application captures");
    let diagnostics = format!(
        "{}\n{provider_capture:?}\n{application_capture:?}",
        debug.lock().expect("debug output").join("\n")
    );
    for sentinel in [
        PROVIDER_HEADER,
        PROVIDER_REQUEST,
        PROVIDER_RESPONSE,
        APPLICATION_TOKEN,
    ] {
        assert!(!diagnostics.contains(sentinel));
    }
}
