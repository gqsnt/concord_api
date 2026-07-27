use super::*;

#[tokio::test]
async fn production_managed_reqwest_inserts_auth_over_real_tls_loopback() {
    const NATIVE_ONLY: &str = "real-loopback-auth-91f4867e0bc2";
    let server = TlsServer::start().expect("TLS loopback server");
    let hooks = Arc::new(AuthHooks::default());
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = ClientCore::build(
        &server.url("localhost", "/v1/"),
        HttpConfigBuilder::new()
            .http1_only()
            .add_root_certificate_pem(server.certificate_pem()),
        RuntimeConfigBuilder::new()
            .hooks(hooks.clone())
            .debug(DebugLevel::Headers),
    )
    .expect("managed TLS core")
    .with_debug_output(Arc::clone(&debug));
    let selector = SlotSelector::static_value(access_token(NATIVE_ONLY), Duration::ZERO);

    let output = request(
        core,
        &selector,
        ChallengePolicy::Never,
        RequestBody::bytes(Bytes::new()),
    )
    .await
    .expect("real managed authenticated request");
    assert_eq!(output, Bytes::from_static(b"ok"));
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("managed request reached TLS server");
    let requests = server.requests();
    assert_eq!(requests[0].target(), "/v1/protected");
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer real-loopback-auth-91f4867e0bc2")
    );

    let diagnostics = format!(
        "{}\n{:?}\n{:?}",
        debug.lock().expect("debug output").join("\n"),
        hooks.before.lock().expect("before records").len(),
        hooks.response.lock().expect("response records").len(),
    );
    assert!(!diagnostics.contains(NATIVE_ONLY));
}

#[tokio::test]
async fn basic_and_custom_header_material_exist_only_at_the_native_boundary() {
    const API_KEY: &str = "header-auth-868e46f4d604";
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url("https://protected.example.test/v1/basic")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Basic dXNlcjpwYXNzd29yZA=="),
                ),
            ScriptedResponse::success(Bytes::from_static(b"basic")),
        )
        .expect("Basic response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url("https://protected.example.test/v1/header")
                .header(
                    http::HeaderName::from_static("x-api-key"),
                    HeaderValue::from_static(API_KEY),
                ),
            ScriptedResponse::success(Bytes::from_static(b"header")),
        )
        .expect("header response");
    let hooks = Arc::new(AuthHooks::default());
    let (core, debug) = core(&executor, hooks);
    let basic = SlotSelector::static_value(basic_credential("user", "password"), Duration::ZERO);
    let key = SlotSelector::static_value(api_key(API_KEY), Duration::ZERO);

    let basic_result = Request::new(
        core.clone(),
        BasicEndpoint {
            credential: CredentialRef::new(basic),
        },
    )
    .await
    .expect("Basic request");
    let header_result = Request::new(
        core,
        HeaderEndpoint {
            credential: CredentialRef::new(key),
        },
    )
    .await
    .expect("header request");
    assert_eq!(basic_result, Bytes::from_static(b"basic"));
    assert_eq!(header_result, Bytes::from_static(b"header"));

    let before = debug.lock().expect("debug output").join("\n");
    assert!(!before.contains("dXNlcjpwYXNzd29yZA=="));
    assert!(!before.contains(API_KEY));
    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    assert!(captures.iter().all(|capture| {
        capture
            .safe_url()
            .is_some_and(|url| !url.contains(API_KEY) && !url.contains("dXNlcjpwYXNzd29yZA=="))
            && capture
                .headers()
                .iter()
                .all(|header| header.value() == &CapturedHeaderValue::Redacted)
    }));
}

#[tokio::test]
async fn prepared_bearer_basic_header_and_query_auth_use_only_the_native_boundary() {
    const BEARER: &str = "prepared-bearer-69da8bc87d53";
    const API_KEY: &str = "prepared-api-key-f22b7940bd61";
    const EXACT_AUTHORIZATION: &str = "Exact prepared authorization 4cf90a";
    const SCHEME_AUTHORIZATION: &str = "prepared-scheme-authorization-36e81b";
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url("https://protected.example.test/v1/bearer")
                .native_url("https://protected.example.test/v1/bearer")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer prepared-bearer-69da8bc87d53"),
                ),
            ScriptedResponse::success(Bytes::from_static(b"bearer")),
        )
        .expect("prepared Bearer response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url("https://protected.example.test/v1/basic")
                .native_url("https://protected.example.test/v1/basic")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("Basic dXNlcjpwYXNzd29yZA=="),
                ),
            ScriptedResponse::success(Bytes::from_static(b"basic")),
        )
        .expect("prepared Basic response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url("https://protected.example.test/v1/header")
                .native_url("https://protected.example.test/v1/header")
                .header(
                    http::HeaderName::from_static("x-api-key"),
                    HeaderValue::from_static(API_KEY),
                ),
            ScriptedResponse::success(Bytes::from_static(b"header")),
        )
        .expect("prepared header response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url("https://protected.example.test/v1/query")
                .native_url(
                    "https://protected.example.test/v1/query?access=prepared-api-key-f22b7940bd61",
                ),
            ScriptedResponse::success(Bytes::from_static(b"query")),
        )
        .expect("prepared query response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url("https://protected.example.test/v1/authorization-exact")
                .native_url("https://protected.example.test/v1/authorization-exact")
                .header(AUTHORIZATION, HeaderValue::from_static(EXACT_AUTHORIZATION)),
            ScriptedResponse::success(Bytes::from_static(b"authorization-exact")),
        )
        .expect("prepared exact Authorization response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url("https://protected.example.test/v1/authorization-scheme")
                .native_url("https://protected.example.test/v1/authorization-scheme")
                .header(
                    AUTHORIZATION,
                    HeaderValue::from_static("tOkEn prepared-scheme-authorization-36e81b"),
                ),
            ScriptedResponse::success(Bytes::from_static(b"authorization-scheme")),
        )
        .expect("prepared scheme Authorization response");
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let bearer = SlotSelector::static_value(access_token(BEARER), Duration::ZERO);
    let basic = SlotSelector::static_value(basic_credential("user", "password"), Duration::ZERO);
    let key = SlotSelector::static_value(api_key(API_KEY), Duration::ZERO);
    let exact = SlotSelector::static_value(api_key(EXACT_AUTHORIZATION), Duration::ZERO);
    let scheme = SlotSelector::static_value(access_token(SCHEME_AUTHORIZATION), Duration::ZERO);

    let cases = [
        (
            "bearer",
            Auth::bearer(CredentialRef::new(Arc::clone(&bearer))),
            Bytes::from_static(b"bearer"),
        ),
        (
            "basic",
            Auth::basic(CredentialRef::new(basic)),
            Bytes::from_static(b"basic"),
        ),
        (
            "header",
            Auth::header(
                http::HeaderName::from_static("x-api-key"),
                CredentialRef::new(Arc::clone(&key)),
            )
            .expect("valid header placement"),
            Bytes::from_static(b"header"),
        ),
        (
            "query",
            Auth::query("access", CredentialRef::new(key)).expect("valid query placement"),
            Bytes::from_static(b"query"),
        ),
        (
            "authorization-exact",
            Auth::authorization(CredentialRef::new(exact)),
            Bytes::from_static(b"authorization-exact"),
        ),
        (
            "authorization-scheme",
            Auth::authorization_with_scheme("tOkEn", CredentialRef::new(scheme))
                .expect("valid static Authorization scheme"),
            Bytes::from_static(b"authorization-scheme"),
        ),
    ];
    for (segment, auth, expected) in cases {
        let endpoint =
            RuntimeEndpoint::builder(EndpointId::from_static("runtime.auth"), Method::GET)
                .path_segment(segment)
                .auth(auth)
                .response_bytes()
                .build()
                .expect("prepared authentication endpoint");
        let actual = Request::new(core.clone(), endpoint)
            .await
            .expect("prepared authenticated request");
        assert_eq!(actual, expected);
    }

    let diagnostics = format!(
        "{}\n{:?}",
        debug.lock().expect("prepared auth debug").join("\n"),
        executor
            .application()
            .captures()
            .expect("prepared auth captures")
    );
    for secret in [
        BEARER,
        API_KEY,
        EXACT_AUTHORIZATION,
        SCHEME_AUTHORIZATION,
        "dXNlcjpwYXNzd29yZA==",
    ] {
        assert!(!diagnostics.contains(secret));
    }
}

#[tokio::test]
async fn prepared_authentication_attachments_materialize_in_builder_order() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .header(
                    http::HeaderName::from_static("x-first-auth"),
                    HeaderValue::from_static("first-prepared-auth-1f31c4"),
                )
                .header(
                    http::HeaderName::from_static("x-second-auth"),
                    HeaderValue::from_static("second-prepared-auth-b4f28e"),
                ),
            ScriptedResponse::success(Bytes::new()),
        )
        .expect("ordered prepared authentication response");
    let (core, debug) = core(&executor, Arc::new(AuthHooks::default()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let first = SlotSelector::static_value(
        OrderedPreparedCredential {
            id: "first",
            value: secret_string("first-prepared-auth-1f31c4"),
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let second = SlotSelector::static_value(
        OrderedPreparedCredential {
            id: "second",
            value: secret_string("second-prepared-auth-b4f28e"),
            calls: Arc::clone(&calls),
        },
        Duration::ZERO,
    );
    let endpoint =
        RuntimeEndpoint::builder(EndpointId::from_static("runtime.ordered_auth"), Method::GET)
            .auth(
                Auth::header(
                    http::HeaderName::from_static("x-first-auth"),
                    CredentialRef::new(first),
                )
                .expect("valid first header placement"),
            )
            .auth(
                Auth::header(
                    http::HeaderName::from_static("x-second-auth"),
                    CredentialRef::new(second),
                )
                .expect("valid second header placement"),
            )
            .response_bytes()
            .build()
            .expect("ordered prepared authentication endpoint");

    Request::new(core, endpoint)
        .await
        .expect("ordered prepared authentication request");
    assert_eq!(
        calls.lock().expect("ordered prepared calls").as_slice(),
        ["first", "second"]
    );
    let debug = debug.lock().expect("ordered prepared debug").join("\n");
    assert!(!debug.contains("first-prepared-auth-1f31c4"));
    assert!(!debug.contains("second-prepared-auth-b4f28e"));
}
