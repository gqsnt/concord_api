use super::*;

static CHALLENGE_PRECEDENCE_ACCEPTANCE_CALLS: AtomicUsize = AtomicUsize::new(0);
static NEVER_CHALLENGE_ACCEPTANCE_CALLS: AtomicUsize = AtomicUsize::new(0);

struct ChallengePrecedenceAcceptance;

impl ResponseBodySpec for ChallengePrecedenceAcceptance {
    type Output = Bytes;
    type Mode = Buffered;

    fn accepts_status(status: StatusCode) -> bool {
        CHALLENGE_PRECEDENCE_ACCEPTANCE_CALLS.fetch_add(1, Ordering::SeqCst);
        status == StatusCode::UNAUTHORIZED
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }
}

struct NeverChallengeAcceptance;

impl ResponseBodySpec for NeverChallengeAcceptance {
    type Output = Bytes;
    type Mode = Buffered;

    fn accepts_status(status: StatusCode) -> bool {
        NEVER_CHALLENGE_ACCEPTANCE_CALLS.fetch_add(1, Ordering::SeqCst);
        status == StatusCode::UNAUTHORIZED
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }
}

#[tokio::test]
async fn attributable_challenge_precedes_declared_status_acceptance() {
    CHALLENGE_PRECEDENCE_ACCEPTANCE_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"attributable-challenge"),
        ))
        .expect("attributable challenge response");
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::from_static("runtime.challenge_precedence"),
        Method::GET,
    )
    .path_segment("challenge-precedence")
    .auth(
        Auth::bearer(CredentialRef::new(Arc::clone(&selector)))
            .challenge(ChallengePolicy::Unauthorized),
    )
    .response::<ChallengePrecedenceAcceptance>()
    .build()
    .expect("challenge precedence endpoint");

    let error = Request::new(core, endpoint)
        .await
        .expect_err("attributable challenge remains authoritative");
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(
        CHALLENGE_PRECEDENCE_ACCEPTANCE_CALLS.load(Ordering::SeqCst),
        0
    );
    assert!(!selector.is_set());
}

#[tokio::test]
async fn challenge_never_allows_the_response_specification_to_accept_401() {
    NEVER_CHALLENGE_ACCEPTANCE_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"accepted-unauthorized"),
        ))
        .expect("accepted unauthorized response");
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::from_static("runtime.challenge_never_acceptance"),
        Method::GET,
    )
    .path_segment("challenge-never-acceptance")
    .auth(Auth::bearer(CredentialRef::new(Arc::clone(&selector))).challenge(ChallengePolicy::Never))
    .response::<NeverChallengeAcceptance>()
    .build()
    .expect("challenge-never acceptance endpoint");

    let response = Request::new(core, endpoint)
        .response()
        .await
        .expect("declared accepted 401");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.value(),
        &Bytes::from_static(b"accepted-unauthorized")
    );
    assert_eq!(response.metadata().attempts_started(), 1);
    assert_eq!(response.metadata().policy_retries(), 0);
    assert_eq!(NEVER_CHALLENGE_ACCEPTANCE_CALLS.load(Ordering::SeqCst), 1);
    assert!(selector.is_set());
}

#[tokio::test]
async fn real_tls_authentication_recovery_performs_two_visible_limiter_acquisitions() {
    let server = TlsServer::start().expect("TLS recovery loopback server");
    server.queue_status(401).expect("challenge status");
    server.queue_status(200).expect("success status");

    let provider_acquires = Arc::new(AtomicUsize::new(0));
    let provider_refreshes = Arc::new(AtomicUsize::new(0));
    let exact_current = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&provider_acquires),
            refreshes: Arc::clone(&provider_refreshes),
            exact_current_seen: Arc::clone(&exact_current),
        },
        Duration::ZERO,
    );
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let core = ClientCore::build(
        &server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(server.certificate_pem()),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    )
    .expect("managed TLS recovery core");

    let output = request(
        core,
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("real loopback recovery");
    assert_eq!(output, Bytes::from_static(b"ok"));
    server
        .wait_for_requests(2, Duration::from_secs(2))
        .expect("both recovery attempts reached TLS loopback");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer initial-auth-token-1148c7")
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer refreshed-auth-token-709fe2")
    );
    assert_eq!(provider_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(provider_refreshes.load(Ordering::SeqCst), 1);
    assert!(exact_current.load(Ordering::SeqCst));
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn paginated_tls_recovery_reuses_lexical_plan_across_recovery_and_later_page() {
    let server = TlsServer::start().expect("TLS pagination recovery server");
    server.queue_status(401).expect("challenge status");
    server.queue_status(200).expect("recovered page status");
    server.queue_status(200).expect("later page status");

    let provider_acquires = Arc::new(AtomicUsize::new(0));
    let provider_refreshes = Arc::new(AtomicUsize::new(0));
    let exact_current = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&provider_acquires),
            refreshes: Arc::clone(&provider_refreshes),
            exact_current_seen: Arc::clone(&exact_current),
        },
        Duration::ZERO,
    );
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let plans = Arc::new(Mutex::new(Vec::new()));
    let limiter: Arc<dyn RateLimiter> = Arc::new(AuthRateLimiter::capturing(
        &limiter_acquires,
        &limiter_observations,
        &plans,
    ));
    let hooks = Arc::new(AuthHooks::default());
    let core = ClientCore::build(
        &server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .rate_limiter(limiter),
    )
    .expect("managed TLS pagination core");

    let items = PaginatedRequest::new(
        Request::new(
            core,
            PaginatedProtectedEndpoint {
                credential: CredentialRef::new(Arc::clone(&selector)),
                page: 0,
            },
        ),
        PageLimit::Take(2),
    )
    .await
    .expect("paginated authentication recovery");
    assert_eq!(items, b"okok");
    server
        .wait_for_requests(3, Duration::from_secs(2))
        .expect("recovery and later-page attempts reached TLS loopback");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer initial-auth-token-1148c7")
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer refreshed-auth-token-709fe2")
    );
    assert_eq!(
        requests[2].header("authorization"),
        Some("Bearer refreshed-auth-token-709fe2")
    );
    assert_eq!(provider_acquires.load(Ordering::SeqCst), 1);
    assert_eq!(provider_refreshes.load(Ordering::SeqCst), 1);
    assert!(exact_current.load(Ordering::SeqCst));
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 3);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 3);
    let captured_plans = plans.lock().expect("captured pagination recovery plans");
    assert_eq!(captured_plans.len(), 3);
    assert_eq!(captured_plans[0].rules, captured_plans[1].rules);
    assert_eq!(
        captured_plans
            .iter()
            .map(|plan| plan.page_index)
            .collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert_eq!(
        captured_plans
            .iter()
            .map(|plan| plan.attempt)
            .collect::<Vec<_>>(),
        [
            INITIAL_ATTEMPT,
            AUTHENTICATION_RECOVERY_ATTEMPT,
            INITIAL_ATTEMPT,
        ]
    );
    for plan in captured_plans.iter() {
        assert_eq!(plan.rules.len(), 1);
        assert_eq!(plan.rules[0].0, "AuthenticationApi::rate_limit::pagination");
        assert!(plan.rules[0].1.is_empty());
        assert_eq!(plan.rules[0].2, 1);
        assert_eq!(plan.rules[0].3, [(1, Duration::from_secs(1))]);
    }
    assert_eq!(captured_plans[0].rules, captured_plans[2].rules);
    drop(captured_plans);

    let before = hooks.before.lock().expect("before records").clone();
    assert_eq!(before.len(), 3);
    assert_eq!(before[0].operation_id, before[1].operation_id);
    assert_ne!(before[0].operation_id, before[2].operation_id);
    assert_eq!(before[0].parent_operation_id, before[1].parent_operation_id);
    assert_eq!(before[0].parent_operation_id, before[2].parent_operation_id);
    assert!(before[0].parent_operation_id.is_some());
    assert_eq!(
        [
            before[0].page_index,
            before[1].page_index,
            before[2].page_index
        ],
        [0, 0, 1]
    );
    assert_eq!(
        [before[0].attempt, before[1].attempt, before[2].attempt,],
        [
            INITIAL_ATTEMPT,
            AUTHENTICATION_RECOVERY_ATTEMPT,
            INITIAL_ATTEMPT,
        ]
    );

    let responses = hooks.response.lock().expect("response records");
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0].0, responses[1].0);
    assert_ne!(responses[0].0, responses[2].0);
    assert_eq!(
        [responses[0].1, responses[1].1, responses[2].1],
        [
            INITIAL_ATTEMPT,
            AUTHENTICATION_RECOVERY_ATTEMPT,
            INITIAL_ATTEMPT,
        ]
    );
    assert_eq!([responses[0].4, responses[1].4, responses[2].4], [0, 0, 1]);
    assert_eq!(responses[0].3, responses[1].3);
    assert_eq!(responses[0].3, responses[2].3);
    drop(responses);

    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 2);
    assert_eq!(completion[0].0, before[0].operation_id);
    assert!(completion[0].1);
    assert_eq!(completion[0].2, Some(StatusCode::OK));
    assert_eq!(completion[0].3, before[0].parent_operation_id);
    assert_eq!(completion[0].4, 0);
    assert_eq!((completion[0].6, completion[0].7), (2, 0));
    assert_eq!(completion[1].0, before[2].operation_id);
    assert!(!completion[1].1);
    assert_eq!(completion[1].2, Some(StatusCode::OK));
    assert_eq!(completion[1].3, before[2].parent_operation_id);
    assert_eq!(completion[1].4, 1);
    assert_eq!((completion[1].6, completion[1].7), (1, 0));
    drop(completion);

    let pagination = hooks.pagination.lock().expect("pagination records");
    assert_eq!(pagination.len(), 1);
    assert_eq!(Some(pagination[0].0), before[0].parent_operation_id);
    assert_eq!(pagination[0].1, 2);
    assert_eq!(pagination[0].2, 4);
    assert!(pagination[0].3);
    assert_eq!(pagination[0].4, Some(StatusCode::OK));
    assert_eq!(pagination[0].5, CompletionOutcome::Success);
    assert_eq!((pagination[0].6, pagination[0].7), (3, 0));
}

#[tokio::test]
async fn paginated_authentication_failure_keeps_its_ordinary_category_and_correlated_completion() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(AuthHooks::default());
    let (core, _debug) = core(&executor, Arc::clone(&hooks));
    let selector = SlotSelector::provider(FailingProvider, Duration::ZERO);

    let error = api_error(
        PaginatedRequest::new(
            Request::new(
                core,
                PaginatedProtectedEndpoint {
                    credential: CredentialRef::new(Arc::clone(&selector)),
                    page: 0,
                },
            ),
            PageLimit::Take(1),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(error.pagination_kind(), None);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    let before = hooks.before.lock().expect("before records");
    assert_eq!(before.len(), 1);
    assert!(before[0].parent_operation_id.is_some());
    let page_operation_id = before[0].operation_id;
    let aggregate_operation_id = before[0]
        .parent_operation_id
        .expect("pagination parent operation");
    drop(before);

    assert!(hooks.response.lock().expect("response records").is_empty());
    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 1);
    assert_eq!(completion[0].0, page_operation_id);
    assert_eq!(completion[0].2, None);
    assert_eq!(completion[0].3, Some(aggregate_operation_id));
    assert_eq!(completion[0].4, 0);
    assert_eq!(completion[0].5, CompletionOutcome::AuthenticationProvider);
    assert_eq!((completion[0].6, completion[0].7), (1, 0));
    drop(completion);

    let pagination = hooks.pagination.lock().expect("pagination records");
    assert_eq!(pagination.len(), 1);
    assert_eq!(pagination[0].0, aggregate_operation_id);
    assert_eq!(pagination[0].1, 0);
    assert_eq!(pagination[0].2, 0);
    assert_eq!(pagination[0].4, None);
    assert_eq!(pagination[0].5, CompletionOutcome::AuthenticationProvider);
    assert_eq!((pagination[0].6, pagination[0].7), (1, 0));
}

#[tokio::test]
async fn challenge_recovery_still_observes_each_received_response_once() {
    const CHALLENGE_BODY: &[u8] = &[b'x'; 70_000];
    let executor = DeterministicExecutor::new();
    let challenge_body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://protected.example.test/v1/protected")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .body(Bytes::from_static(b"buffered")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(CHALLENGE_BODY))
                .body_poll_counter(Arc::clone(&challenge_body_polls)),
        )
        .expect("challenge response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://protected.example.test/v1/protected")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
                )
                .body(Bytes::from_static(b"buffered")),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("recovery response");

    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let exact = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::clone(&exact),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&selector));
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter_ready = Arc::new(Mutex::new(Vec::new()));
    let mut limiter = AuthRateLimiter::counting(&limiter_acquires, &limiter_observations);
    limiter.observed_selector = Some((Arc::clone(&selector), Arc::clone(&limiter_ready)));
    let limiter = Arc::new(limiter);
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );

    let response = request(
        core.clone(),
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::from_static(b"buffered")),
    )
    .response()
    .await
    .expect("one bounded recovery");
    assert_eq!(response.value(), &Bytes::from_static(b"ok"));
    assert!(response.metadata().recovered_authentication());
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 0);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(exact.load(Ordering::SeqCst));
    assert!(
        hooks
            .observed_ready_during_response_hook
            .load(Ordering::SeqCst)
    );
    assert_eq!(core.operation_allocations(), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(executor.provider().request_count().expect("count"), 0);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 2);
    assert_eq!(
        limiter_ready.lock().expect("limiter readiness").as_slice(),
        [true, true],
        "limiter observation precedes challenge mutation on the first response"
    );
    assert_eq!(challenge_body_polls.load(Ordering::SeqCst), 0);

    let before = hooks.before.lock().expect("before records").clone();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].operation_id, before[1].operation_id);
    assert_eq!(
        [before[0].attempt, before[1].attempt],
        [INITIAL_ATTEMPT, AUTHENTICATION_RECOVERY_ATTEMPT]
    );
    for record in before {
        assert_eq!(
            record.safe_url,
            "https://protected.example.test/v1/protected"
        );
        assert!(!record.safe_url.contains(INITIAL));
        assert!(!record.safe_url.contains(REFRESHED));
    }
    let responses = hooks.response.lock().expect("response records");
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0].0, responses[1].0);
    assert_eq!(
        [responses[0].1, responses[1].1],
        [INITIAL_ATTEMPT, AUTHENTICATION_RECOVERY_ATTEMPT]
    );
    drop(responses);
    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 1);
    assert_eq!(completion[0].0, before_operation_id(&hooks));
    assert!(completion[0].1);
    assert_eq!(completion[0].2, Some(StatusCode::OK));
    assert_eq!((completion[0].6, completion[0].7), (2, 0));

    let rendered = debug.lock().expect("debug output").join("\n");
    assert!(!rendered.contains(INITIAL));
    assert!(!rendered.contains(REFRESHED));
    let captures = executor.application().captures().expect("captures");
    assert!(captures.iter().all(|capture| {
        capture.safe_url() == Some("https://protected.example.test/v1/protected")
            && capture.headers().iter().any(|header| {
                header.name() == AUTHORIZATION && header.value() == &CapturedHeaderValue::Redacted
            })
            && !format!("{capture:?}").contains(INITIAL)
            && !format!("{capture:?}").contains(REFRESHED)
    }));
}

#[tokio::test]
async fn response_bytes_preserves_authentication_recovery_and_logical_native_url_separation() {
    let executor = DeterministicExecutor::new();
    let challenge_body_polls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://protected.example.test/v1/protected")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .body(Bytes::from_static(b"raw-auth-body")),
            ScriptedResponse::status(
                StatusCode::UNAUTHORIZED,
                Bytes::from_static(b"raw-auth-challenge-secret"),
            )
            .body_poll_counter(Arc::clone(&challenge_body_polls)),
        )
        .expect("raw challenge response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url("https://protected.example.test/v1/protected")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
                )
                .body(Bytes::from_static(b"raw-auth-body")),
            ScriptedResponse::success(Bytes::from_static(b"raw-auth-success-secret")),
        )
        .expect("raw recovery response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url(
                    "https://protected.example.test/v1/protected?access=refreshed-auth-token-709fe2",
                )
                .body(Bytes::new()),
            ScriptedResponse::success(Bytes::from_static(b"raw-query-success")),
        )
        .expect("raw query-auth response");

    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let exact = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::clone(&exact),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&selector));
    let (core, debug) = core(&executor, Arc::clone(&hooks));

    let response = request(
        core.clone(),
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::from_static(b"raw-auth-body")),
    )
    .response_bytes()
    .await
    .expect("response bytes after one authentication recovery");
    assert_eq!(response.value(), b"raw-auth-success-secret".as_slice());
    assert_eq!(
        response.url().as_str(),
        "https://protected.example.test/v1/protected"
    );
    assert!(response.metadata().recovered_authentication());
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 0);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(exact.load(Ordering::SeqCst));
    assert_eq!(challenge_body_polls.load(Ordering::SeqCst), 0);

    let query_response = query_request(core.clone(), &selector)
        .response_bytes()
        .await
        .expect("response-bytes query-auth response");
    assert_eq!(query_response.value(), b"raw-query-success".as_slice());
    assert_eq!(
        query_response.url().as_str(),
        "https://protected.example.test/v1/protected"
    );
    assert!(!query_response.url().as_str().contains("access="));
    assert_eq!(core.operation_allocations(), 2);
    assert_eq!(executor.application().request_count().expect("count"), 3);

    let before = hooks.before.lock().expect("before records");
    assert_eq!(before.len(), 3);
    assert_eq!(
        [before[0].attempt, before[1].attempt, before[2].attempt],
        [
            INITIAL_ATTEMPT,
            AUTHENTICATION_RECOVERY_ATTEMPT,
            INITIAL_ATTEMPT,
        ]
    );
    assert!(before.iter().all(|record| {
        !record.safe_url.contains(INITIAL)
            && !record.safe_url.contains(REFRESHED)
            && !record.safe_url.contains("access=")
    }));
    drop(before);
    let completion = hooks.completion.lock().expect("completion records");
    assert_eq!(completion.len(), 2);
    assert!(completion.iter().all(|record| {
        record.2 == Some(StatusCode::OK) && record.5 == CompletionOutcome::Success
    }));
    assert_eq!(
        completion
            .iter()
            .map(|record| (record.6, record.7))
            .collect::<Vec<_>>(),
        [(2, 0), (1, 0)]
    );
    drop(completion);
    let automatic = format!(
        "{}\n{response:?}\n{query_response:?}",
        debug.lock().expect("debug output").join("\n")
    );
    for forbidden in [
        INITIAL,
        REFRESHED,
        "raw-auth-success-secret",
        "raw-query-success",
        "access=",
    ] {
        assert!(
            !automatic.contains(forbidden),
            "automatic surface leaked {forbidden}"
        );
    }
}

#[tokio::test]
async fn prepared_codec_recovery_reuses_encoding_and_reenters_the_ordinary_limiter_lifecycle() {
    const PUBLIC_QUERY: &str = "prepared-public-query-3fa67d2cae91";
    const PUBLIC_HEADER: &str = "prepared-public-header-86d22d942a6b";
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url(
                    "https://protected.example.test/v1/prepared?session_token=prepared-public-query-3fa67d2cae91",
                )
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
                )
                .header(
                    http::HeaderName::from_static("x-sensitive-token"),
                    HeaderValue::from_static(PUBLIC_HEADER),
                )
                .body(Bytes::from_static(b"prepared-codec")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"challenge")),
        )
        .expect("prepared challenge response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .native_url(
                    "https://protected.example.test/v1/prepared?session_token=prepared-public-query-3fa67d2cae91",
                )
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer refreshed-auth-token-709fe2"),
                )
                .header(
                    http::HeaderName::from_static("x-sensitive-token"),
                    HeaderValue::from_static(PUBLIC_HEADER),
                )
                .body(Bytes::from_static(b"prepared-codec")),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("prepared recovery response");

    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let exact = Arc::new(AtomicBool::new(false));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::clone(&exact),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let limiter_acquires = Arc::new(AtomicUsize::new(0));
    let limiter_observations = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(AuthRateLimiter::counting(
        &limiter_acquires,
        &limiter_observations,
    ));
    let (core, debug) = core_with_runtime(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().rate_limiter(limiter),
    );
    let encode_calls = Arc::new(AtomicUsize::new(0));
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.protected"), Method::POST)
            .path_segment("prepared")
            .query("session_token", PUBLIC_QUERY)
            .header(
                http::HeaderName::from_static("x-sensitive-token"),
                PUBLIC_HEADER,
            )
            .body_codec::<PreparedRecoveryCodec>(PreparedRecoveryInput {
                calls: Arc::clone(&encode_calls),
            })
            .auth(
                Auth::bearer(CredentialRef::new(Arc::clone(&selector)))
                    .challenge(ChallengePolicy::Unauthorized),
            )
            .rate_limit(prepared_rate_rule())
            .response_bytes()
            .build()
            .expect("prepared protected endpoint");

    let response = Request::new(core.clone(), endpoint)
        .response()
        .await
        .expect("prepared bounded recovery");
    assert_eq!(response.value(), &Bytes::from_static(b"ok"));
    assert!(response.metadata().recovered_authentication());
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 0);
    assert_eq!(encode_calls.load(Ordering::SeqCst), 1);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(exact.load(Ordering::SeqCst));
    assert_eq!(core.operation_allocations(), 1);
    assert_eq!(limiter_acquires.load(Ordering::SeqCst), 2);
    assert_eq!(limiter_observations.load(Ordering::SeqCst), 2);

    let before = hooks.before.lock().expect("prepared before records");
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].operation_id, before[1].operation_id);
    assert_eq!(
        [before[0].attempt, before[1].attempt],
        [INITIAL_ATTEMPT, AUTHENTICATION_RECOVERY_ATTEMPT]
    );
    assert!(before.iter().all(|record| {
        record.safe_url == "https://protected.example.test/v1/prepared?session_token=%3Credacted%3E"
            && !record.safe_url.contains(INITIAL)
            && !record.safe_url.contains(REFRESHED)
            && !record.safe_url.contains(PUBLIC_QUERY)
    }));
    drop(before);
    let completion = hooks.completion.lock().expect("prepared completion");
    assert_eq!(completion.len(), 1);
    assert!(completion[0].1);
    assert_eq!(completion[0].2, Some(StatusCode::OK));
    assert_eq!((completion[0].6, completion[0].7), (2, 0));
    drop(completion);
    let diagnostics = format!(
        "{}\n{:?}",
        debug.lock().expect("prepared debug output").join("\n"),
        executor
            .application()
            .captures()
            .expect("prepared captures")
    );
    for sentinel in [
        INITIAL,
        REFRESHED,
        PUBLIC_QUERY,
        PUBLIC_HEADER,
        "prepared-codec",
    ] {
        assert!(!diagnostics.contains(sentinel));
    }
}

#[tokio::test]
async fn prepared_one_shot_body_does_not_perform_an_unsafe_recovery_resend() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"prepared-one-shot")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"terminal")),
        )
        .expect("prepared one-shot challenge");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.one_shot"), Method::POST)
            .body(RequestBody::stream(StreamBody::from_bytes(
                Bytes::from_static(b"prepared-one-shot"),
            )))
            .auth(
                Auth::bearer(CredentialRef::new(selector)).challenge(ChallengePolicy::Unauthorized),
            )
            .response_bytes()
            .build()
            .expect("prepared one-shot endpoint");

    let error = api_error(Request::new(core, endpoint).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn prepared_one_shot_multipart_does_not_perform_an_unsafe_recovery_resend() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"terminal"),
        ))
        .expect("prepared multipart challenge");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let multipart = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(
                b"prepared-multipart-one-shot",
            ))),
        )
        .expect("prepared one-shot multipart");
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::from_static("runtime.multipart_one_shot"),
        Method::POST,
    )
    .body(RequestBody::multipart(multipart))
    .auth(Auth::bearer(CredentialRef::new(selector)).challenge(ChallengePolicy::Unauthorized))
    .response_bytes()
    .build()
    .expect("prepared one-shot multipart endpoint");

    let error = api_error(Request::new(core, endpoint).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn recovered_stream_reports_recovery_in_owned_metadata() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"unread"),
        ))
        .expect("challenge response");
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"streamed")).headers(headers))
        .expect("stream response");
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::new(AtomicUsize::new(0)),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let response = Request::new(
        core,
        ProtectedStreamEndpoint {
            credential: CredentialRef::new(selector),
        },
    )
    .response()
    .await
    .expect("recovered stream should open");
    assert!(response.metadata().recovered_authentication());
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 0);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn response_hook_replacement_is_observed_before_challenge_and_newer_material_is_reused() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer initial-auth-token-1148c7"),
            ),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"unread")),
        )
        .expect("challenge response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().header(
                AUTHORIZATION,
                HeaderValue::from_static("Bearer newer-auth-token-265f0a"),
            ),
            ScriptedResponse::success(Bytes::from_static(b"newer")),
        )
        .expect("newer response");
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::clone(&acquires),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&selector));
    hooks.replace_on_first_response();
    let (core, _) = core(&executor, Arc::clone(&hooks));

    let result = request(
        core,
        &selector,
        ChallengePolicy::Unauthorized,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("newer current credential recovery");
    assert_eq!(result, Bytes::from_static(b"newer"));
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(
        hooks
            .ready_during_response
            .lock()
            .expect("response readiness")
            .as_slice(),
        &[true, true]
    );
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

fn before_operation_id(hooks: &AuthHooks) -> OperationId {
    hooks.before.lock().expect("before records")[0].operation_id
}

#[tokio::test]
async fn reconstructible_factory_runs_once_per_concord_attempt_and_no_third_attempt_occurs() {
    let executor = DeterministicExecutor::new();
    for token in [INITIAL, REFRESHED] {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .header(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {token}")).expect("header"),
                    )
                    .body(Bytes::from_static(b"factory-body")),
                ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"terminal")),
            )
            .expect("challenge response");
    }
    let acquires = Arc::new(AtomicUsize::new(0));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires,
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::factory(ReplayFactory {
                calls: Arc::clone(&factory_calls),
            }),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(
        hooks
            .before
            .lock()
            .expect("before records")
            .iter()
            .map(|record| record.known_body_length)
            .collect::<Vec<_>>(),
        [None, None]
    );
}

#[tokio::test]
async fn recovery_hook_veto_is_primary_and_invalidates_exact_challenged_material() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"unread"),
        ))
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let hooks = Arc::new(AuthHooks::default());
    hooks.veto_recovery();
    let (core, _) = core(&executor, hooks);
    let error = api_error(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    let current = selector
        .preflight(core.provider_http(None))
        .await
        .expect("later operation refreshes rejected material");
    assert_eq!(current.value().expose_secret(), REFRESHED);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn recovery_reconstruction_failure_is_primary_and_invalidates_challenged_material() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"initial-body")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"unread")),
        )
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let calls = Arc::new(AtomicUsize::new(0));
    let error = api_error(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::factory(FailOnSecondFactory {
                calls: Arc::clone(&calls),
            }),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    let current = selector
        .preflight(core.provider_http(None))
        .await
        .expect("later operation refreshes rejected material");
    assert_eq!(current.value().expose_secret(), REFRESHED);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn one_shot_body_prevents_recovery_and_provider_refresh() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"one-shot")),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::from_static(b"terminal")),
        )
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let stream = StreamBody::from_bytes(Bytes::from_static(b"one-shot"));
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::stream(stream),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn one_shot_multipart_prevents_recovery_and_provider_refresh() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"terminal"),
        ))
        .expect("challenge response");
    let refreshes = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        RotatingProvider {
            acquires: Arc::new(AtomicUsize::new(0)),
            refreshes: Arc::clone(&refreshes),
            exact_current_seen: Arc::new(AtomicBool::new(false)),
        },
        Duration::ZERO,
    );
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let multipart = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(b"one-shot"))),
        )
        .expect("one-shot multipart");
    let error = api_error(
        request(
            core,
            &selector,
            ChallengePolicy::Unauthorized,
            RequestBody::multipart(multipart),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn static_and_endpoint_challenges_mutate_only_exact_attributed_source_without_resend() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"static terminal"),
        ))
        .expect("static challenge");
    let static_slot = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let first = api_error(
        request(
            core.clone(),
            &static_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(first.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    let second = api_error(
        request(
            core.clone(),
            &static_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(second.auth_kind(), Some(AuthErrorKind::Rejected));
    assert_eq!(executor.application().request_count().expect("count"), 1);
    static_slot
        .replace_static(access_token(REFRESHED))
        .expect("complete static replacement");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"replaced")))
        .expect("replacement response");
    assert_eq!(
        request(
            core.clone(),
            &static_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await
        .expect("replacement clears rejection"),
        Bytes::from_static(b"replaced")
    );

    let endpoint_slot = SlotSelector::endpoint(Duration::ZERO);
    endpoint_slot
        .set(access_token(INITIAL))
        .expect("endpoint material");
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"endpoint terminal"),
        ))
        .expect("endpoint challenge");
    api_error(
        request(
            core,
            &endpoint_slot,
            ChallengePolicy::Unauthorized,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert!(!endpoint_slot.is_set());
    assert_eq!(executor.application().request_count().expect("count"), 3);
}

#[tokio::test]
async fn forbidden_policy_and_single_attribution_leave_unrelated_credentials_unchanged() {
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
                    HeaderValue::from_static(NEWER),
                ),
            ScriptedResponse::status(StatusCode::FORBIDDEN, Bytes::from_static(b"terminal")),
        )
        .expect("attributed forbidden response");
    let first = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let second = SlotSelector::static_value(access_token(NEWER), Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    hooks.observe(Arc::clone(&first));
    let (core, _) = core(&executor, Arc::clone(&hooks));
    let error = api_error(
        Request::new(
            core.clone(),
            MultiAuthEndpoint {
                first: CredentialRef::new(Arc::clone(&first)),
                second: CredentialRef::new(Arc::clone(&second)),
                first_challenge: ChallengePolicy::UnauthorizedOrForbidden,
                body_calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::AuthenticationRejected);
    assert!(!first.is_set());
    assert!(second.is_set());
    assert_eq!(
        hooks
            .ready_during_response
            .lock()
            .expect("response readiness")
            .as_slice(),
        &[true]
    );

    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::FORBIDDEN,
            Bytes::from_static(b"ordinary forbidden"),
        ))
        .expect("ordinary forbidden response");
    let first = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let second = SlotSelector::static_value(access_token(NEWER), Duration::ZERO);
    let error = api_error(
        Request::new(
            core,
            MultiAuthEndpoint {
                first: CredentialRef::new(Arc::clone(&first)),
                second: CredentialRef::new(Arc::clone(&second)),
                first_challenge: ChallengePolicy::Unauthorized,
                body_calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(first.is_set());
    assert!(second.is_set());
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn default_never_preserves_static_material_and_query_auth_exists_only_in_native_url() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::UNAUTHORIZED,
            Bytes::from_static(b"ordinary status"),
        ))
        .expect("ordinary status");
    let selector = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let error = api_error(
        request(
            core.clone(),
            &selector,
            ChallengePolicy::Never,
            RequestBody::bytes(Bytes::new()),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert!(selector.is_set());

    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "https://protected.example.test/v1/protected?access={INITIAL}"
                ))
                .safe_url("https://protected.example.test/v1/protected"),
            ScriptedResponse::success(Bytes::from_static(b"query")),
        )
        .expect("query response");
    assert_eq!(
        query_request(core, &selector)
            .await
            .expect("native query auth"),
        Bytes::from_static(b"query")
    );
    let captures = executor.application().captures().expect("captures");
    let query_capture = captures.last().expect("query capture");
    assert_eq!(
        query_capture.safe_url(),
        Some("https://protected.example.test/v1/protected")
    );
    assert!(!format!("{query_capture:?}").contains(INITIAL));
}
