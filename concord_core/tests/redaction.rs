#[allow(dead_code)]
#[path = "current_core/common.rs"]
mod common;

mod query_auth_redaction {
    use super::common::{MockResponse, MockTransport, auth_policy, decode_string, request_plan};
    use bytes::Bytes;
    use concord_core::advanced::ClientCertificate;
    use concord_core::advanced::{
        AuthApplicationRequest, AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind,
        AuthHttpRequest, AuthInternalPolicy, AuthMode, AuthPlacement, AuthRequirement,
        AuthRequirementId, AuthRetryReason, BuiltRequest, CacheAfter, CacheBefore, CacheFuture,
        CacheStore, DebugSink, PreparedInternalAuth, RequestMeta, RuntimeHooks, Transport,
        TransportAuth, TransportError, TransportErrorHookContext, TransportResponse,
        apply_basic_credential, apply_certificate_credential, apply_secret_credential,
        default_cache_key,
    };
    #[cfg(feature = "json")]
    use concord_core::advanced::{CredentialProvider, OAuth2ClientCredentialsProvider};
    use concord_core::advanced::{PreparedAuthCredential, TransportRequest};
    use concord_core::internal::{ClientPlanContext, RequestPlan, ResolvedPolicy};
    use concord_core::prelude::{
        AccessToken, ApiClient, ApiClientError, ApiKey, BasicCredential, ClientContext, DebugLevel,
        Endpoint,
    };
    use http::{HeaderMap, Method, StatusCode};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::Mutex as TokioMutex;

    const API_KEY_SECRET: &str = "LEAK_SENTINEL_API_KEY_123";
    const BEARER_SECRET: &str = "LEAK_SENTINEL_BEARER_456";
    const BASIC_USERNAME_SECRET: &str = "LEAK_SENTINEL_BASIC_USERNAME_012";
    const PASSWORD_SECRET: &str = "LEAK_SENTINEL_PASSWORD_789";
    const REFRESH_SECRET_A: &str = "LEAK_SENTINEL_REFRESH_A";
    const REFRESH_SECRET_B: &str = "LEAK_SENTINEL_REFRESH_B";
    const CERTIFICATE_ID: &str = "LEAK_SENTINEL_CERTIFICATE_ID";
    const INTERNAL_AUTH_SECRET: &str = "LEAK_SENTINEL_INTERNAL_AUTH";
    #[cfg(feature = "json")]
    const CLIENT_SECRET: &str = "LEAK_SENTINEL_CLIENT_SECRET_ABC";

    fn assert_secret_absent(output: &str, secret: &str) {
        assert!(
            !output.contains(secret),
            "secret leaked in output:\n{output}"
        );
    }

    #[derive(Clone, Debug)]
    struct RedactionAuthVars {
        api_key: String,
        bearer: String,
        username: String,
        password: String,
    }

    #[derive(Clone)]
    struct RedactionCx;

    impl ClientContext for RedactionCx {
        type Vars = ();
        type AuthVars = RedactionAuthVars;
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.com";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

        fn prepare_auth_requirement<'a>(
            requirement: &'a AuthRequirement,
            request: &'a mut AuthApplicationRequest<'_>,
            _vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            _auth_state: &'a Self::AuthState,
            _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
            _meta: &'a RequestMeta,
        ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>>
        {
            Box::pin(async move {
                let application = match requirement.placement {
                    AuthPlacement::Basic => {
                        let material =
                            BasicCredential::new(auth.username.clone(), auth.password.clone());
                        apply_basic_credential(request, requirement, &material)?
                    }
                    AuthPlacement::Bearer => {
                        let material = AccessToken::new(auth.bearer.clone());
                        apply_secret_credential(request, requirement, &material)?
                    }
                    AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                        if auth.api_key.is_empty() {
                            return Err(AuthError::new(
                                AuthErrorKind::MissingCredential,
                                format!(
                                    "missing credential `{}` for auth usage `{}`",
                                    requirement.credential.id, requirement.usage_id
                                ),
                            ));
                        }
                        let material = ApiKey::new(auth.api_key.clone());
                        apply_secret_credential(request, requirement, &material)?
                    }
                    AuthPlacement::Certificate => {
                        return Err(AuthError::new(
                            AuthErrorKind::UnsupportedScheme,
                            "redaction test context does not use certificate auth",
                        ));
                    }
                };
                let applied = AuthAppliedCredential {
                    credential_id: requirement.credential.id.clone(),
                    usage_id: requirement.usage_id.clone(),
                    step_id: requirement.step_id,
                    generation: Some(1),
                    identity: application.identity().clone(),
                    provenance: requirement.provenance.clone(),
                };
                Ok(PreparedAuthCredential::new(applied, application))
            })
        }

        fn handle_auth_response<'a>(
            _requirement: &'a AuthRequirement,
            _applied: &'a AuthAppliedCredential,
            _vars: &'a Self::Vars,
            _auth: &'a Self::AuthVars,
            _auth_state: &'a Self::AuthState,
            _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
            _meta: &'a RequestMeta,
            _status: StatusCode,
            _headers: &'a HeaderMap,
        ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
            Box::pin(async { Ok(AuthDecision::Continue) })
        }
    }

    #[derive(Clone)]
    struct RedactionEndpoint {
        policy: ResolvedPolicy,
    }

    impl Endpoint<RedactionCx> for RedactionEndpoint {
        type Response = String;

        fn plan(
            &self,
            _ctx: &ClientPlanContext<'_, RedactionCx>,
        ) -> Result<RequestPlan, ApiClientError> {
            Ok(request_plan(
                "Redaction",
                Method::GET,
                "/text",
                self.policy.clone(),
                None,
                decode_string,
            ))
        }
    }

    #[derive(Default)]
    struct UrlDebugSink {
        events: Mutex<Vec<String>>,
    }

    impl UrlDebugSink {
        fn events(&self) -> Vec<String> {
            self.events.lock().expect("debug events lock").clone()
        }
    }

    impl DebugSink for UrlDebugSink {
        fn request_start(
            &self,
            _dbg: DebugLevel,
            _method: &Method,
            url: &str,
            _endpoint: &'static str,
            _page_index: u32,
        ) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("request:{url}"));
        }

        fn request_headers(&self, _dbg: DebugLevel, headers: &HeaderMap) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("request_headers:{headers:?}"));
        }

        fn request_body(
            &self,
            _dbg: DebugLevel,
            _body: &Bytes,
            _format: concord_core::internal::Format,
            _max_chars: usize,
        ) {
        }

        fn response_status(&self, _dbg: DebugLevel, _status: StatusCode, url: &str, ok: bool) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("response:{ok}:{url}"));
        }

        fn response_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

        fn response_body(
            &self,
            _dbg: DebugLevel,
            _body: &Bytes,
            _format: concord_core::internal::Format,
            _max_chars: usize,
        ) {
        }

        fn stale_fallback(
            &self,
            _dbg: DebugLevel,
            _method: &Method,
            url: &str,
            _endpoint: &'static str,
            _page_index: u32,
        ) {
            self.events
                .lock()
                .expect("debug events lock")
                .push(format!("stale:{url}"));
        }
    }

    #[derive(Default)]
    struct TransportErrorHooks {
        events: TokioMutex<Vec<String>>,
    }

    impl TransportErrorHooks {
        async fn events(&self) -> Vec<String> {
            self.events.lock().await.clone()
        }
    }

    impl RuntimeHooks for TransportErrorHooks {
        fn transport_error<'a>(
            &'a self,
            ctx: TransportErrorHookContext<'a>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async move {
                self.events
                    .lock()
                    .await
                    .push(format!("transport_error:{}", ctx.meta.url));
            })
        }
    }

    fn policy_with_query_auth(key: &'static str) -> ResolvedPolicy {
        let mut policy = auth_policy(AuthPlacement::Query(key));
        policy.query.push(("page".to_string(), "2".to_string()));
        policy
    }

    fn redaction_auth_vars() -> RedactionAuthVars {
        RedactionAuthVars {
            api_key: API_KEY_SECRET.to_string(),
            bearer: BEARER_SECRET.to_string(),
            username: BASIC_USERNAME_SECRET.to_string(),
            password: PASSWORD_SECRET.to_string(),
        }
    }

    async fn run_debug_request(
        policy: ResolvedPolicy,
        status: StatusCode,
    ) -> Result<(Vec<String>, Vec<TransportRequest>), ApiClientError> {
        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(events, vec![MockResponse::text(status, "ok")]);
        let sent = transport.clone();
        let mut client =
            ApiClient::<RedactionCx, _>::with_transport((), redaction_auth_vars(), transport);
        let debug = Arc::new(UrlDebugSink::default());
        client.set_debug_sink(debug.clone());

        let request = client
            .request(RedactionEndpoint { policy })
            .debug_level(DebugLevel::VV)
            .execute_decoded()
            .await;

        if status.is_success() {
            request?;
        } else {
            let err = request.expect_err("HTTP error should be returned");
            assert!(err.to_string().contains(status.as_str()));
        }

        Ok((debug.events(), sent.requests().await))
    }

    #[derive(Clone)]
    struct FailingTransport {
        requests: Arc<TokioMutex<Vec<TransportRequest>>>,
    }

    impl FailingTransport {
        fn new() -> Self {
            Self {
                requests: Arc::new(TokioMutex::new(Vec::new())),
            }
        }

        async fn requests(&self) -> Vec<TransportRequest> {
            self.requests.lock().await.clone()
        }
    }

    impl Transport for FailingTransport {
        fn send(
            &self,
            req: TransportRequest,
        ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>>
        {
            let requests = self.requests.clone();
            Box::pin(async move {
                requests.lock().await.push(req);
                Err(TransportError::with_kind(
                    concord_core::advanced::TransportErrorKind::Connect,
                    std::io::Error::other("redaction transport failure"),
                ))
            })
        }
    }

    #[derive(Default)]
    struct RecordingCache {
        observations: TokioMutex<Vec<String>>,
    }

    impl RecordingCache {
        async fn observations(&self) -> Vec<String> {
            self.observations.lock().await.clone()
        }
    }

    impl CacheStore for RecordingCache {
        fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
            Box::pin(async move {
                let mut observations = self.observations.lock().await;
                observations.push(format!("key:{}", default_cache_key(request).as_str()));
                observations.push(format!("request:{request:?}"));
                observations.push(format!(
                    "identities:{:?}",
                    request.extensions.auth_identities
                ));
                CacheBefore::Miss
            })
        }

        fn after_response<'a>(
            &'a self,
            _request: &'a BuiltRequest,
            _response: &'a concord_core::advanced::BuiltResponse,
            _revalidation: Option<concord_core::advanced::CacheRevalidation>,
        ) -> CacheFuture<'a, CacheAfter> {
            Box::pin(async { CacheAfter::Stored })
        }
    }

    async fn run_transport_error_request(
        policy: ResolvedPolicy,
    ) -> Result<(String, Vec<TransportRequest>), ApiClientError> {
        let transport = FailingTransport::new();
        let sent = transport.clone();
        let mut client =
            ApiClient::<RedactionCx, _>::with_transport((), redaction_auth_vars(), transport);
        let hooks = Arc::new(TransportErrorHooks::default());
        client.configure(|cfg| {
            cfg.runtime_hooks(hooks.clone());
        });

        let err = client
            .request(RedactionEndpoint { policy })
            .debug_level(DebugLevel::V)
            .execute_decoded()
            .await
            .expect_err("transport error should be returned");
        let output = format!("{}\n{}", err, hooks.events().await.join("\n"));
        Ok((output, sent.requests().await))
    }

    #[tokio::test]
    async fn debug_url_redacts_query_auth_secret() -> Result<(), ApiClientError> {
        let (events, requests) =
            run_debug_request(policy_with_query_auth("api_key"), StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert_secret_absent(&debug_output, API_KEY_SECRET);
        assert!(debug_output.contains("api_key=<redacted>"));
        assert!(
            requests[0].url.as_str().contains(API_KEY_SECRET),
            "transport URL should retain the real query auth secret"
        );
        Ok(())
    }

    #[tokio::test]
    async fn debug_response_url_redacts_query_auth_secret() -> Result<(), ApiClientError> {
        let (events, requests) = run_debug_request(
            policy_with_query_auth("api_key"),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
        .await?;

        let debug_output = events.join("\n");
        assert_secret_absent(&debug_output, API_KEY_SECRET);
        assert!(
            debug_output
                .contains("response:false:https://example.com/text?page=2&api_key=<redacted>")
        );
        assert!(requests[0].url.as_str().contains(API_KEY_SECRET));
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_preserves_non_sensitive_query_values() -> Result<(), ApiClientError> {
        let (events, _) =
            run_debug_request(policy_with_query_auth("api_key"), StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("page=2"));
        assert!(debug_output.contains("api_key=<redacted>"));
        assert_secret_absent(&debug_output, API_KEY_SECRET);
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_redacts_case_insensitive_sensitive_keys() -> Result<(), ApiClientError> {
        let (events, _) =
            run_debug_request(policy_with_query_auth("API_KEY"), StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("API_KEY=<redacted>"));
        assert_secret_absent(&debug_output, API_KEY_SECRET);
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_redacts_duplicate_sensitive_query_keys() -> Result<(), ApiClientError> {
        let mut policy = policy_with_query_auth("api_key");
        policy
            .query
            .push(("api_key".to_string(), "also-secret".to_string()));
        policy.query.push(("page".to_string(), "2".to_string()));

        let (events, requests) = run_debug_request(policy, StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert!(debug_output.matches("api_key=<redacted>").count() >= 2);
        assert!(debug_output.contains("page=2"));
        assert_secret_absent(&debug_output, API_KEY_SECRET);
        assert_secret_absent(&debug_output, "also-secret");
        assert!(requests[0].url.as_str().contains(API_KEY_SECRET));
        assert!(requests[0].url.as_str().contains("also-secret"));
        Ok(())
    }

    #[tokio::test]
    async fn debug_url_redacts_custom_query_auth_key() -> Result<(), ApiClientError> {
        let (events, requests) = run_debug_request(
            policy_with_query_auth("x-private-provider-key"),
            StatusCode::OK,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("x-private-provider-key=<redacted>"));
        assert!(debug_output.contains("page=2"));
        assert_secret_absent(&debug_output, API_KEY_SECRET);
        assert!(requests[0].url.as_str().contains(API_KEY_SECRET));
        Ok(())
    }

    #[tokio::test]
    async fn arbitrary_query_auth_name_is_structurally_redacted() -> Result<(), ApiClientError> {
        let (events, requests) =
            run_debug_request(policy_with_query_auth("provider"), StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("provider=<redacted>"));
        assert!(debug_output.contains("page=2"));
        assert_secret_absent(&debug_output, API_KEY_SECRET);

        let transport_url = requests[0].url.as_str();
        assert!(
            transport_url.contains("provider=LEAK_SENTINEL_API_KEY_123"),
            "transport request should contain real query auth at send boundary: {transport_url}"
        );
        assert_secret_absent(&format!("{:?}", requests[0]), API_KEY_SECRET);
        Ok(())
    }

    #[tokio::test]
    async fn arbitrary_header_auth_name_is_structurally_redacted() -> Result<(), ApiClientError> {
        let (events, requests) = run_debug_request(
            auth_policy(AuthPlacement::Header("X-Custom")),
            StatusCode::OK,
        )
        .await?;

        let debug_output = events.join("\n");
        assert!(debug_output.contains("request:https://example.com/text"));
        assert_secret_absent(&debug_output, API_KEY_SECRET);

        let header = requests[0]
            .headers
            .get("X-Custom")
            .and_then(|value| value.to_str().ok());
        assert_eq!(header, Some(API_KEY_SECRET));
        assert_secret_absent(&format!("{:?}", requests[0]), API_KEY_SECRET);
        Ok(())
    }

    #[tokio::test]
    async fn debug_urls_do_not_leak_bearer_header_or_basic_auth_secrets()
    -> Result<(), ApiClientError> {
        for (placement, secret, expected) in [
            (
                AuthPlacement::Bearer,
                BEARER_SECRET,
                "request:https://example.com/text",
            ),
            (
                AuthPlacement::Header("X-Api-Key"),
                API_KEY_SECRET,
                "request:https://example.com/text",
            ),
            (
                AuthPlacement::Basic,
                PASSWORD_SECRET,
                "request:https://example.com/text",
            ),
        ] {
            let (events, requests) =
                run_debug_request(auth_policy(placement), StatusCode::OK).await?;
            let debug_output = events.join("\n");
            assert!(debug_output.contains(expected));
            assert_secret_absent(&debug_output, secret);
            assert!(
                !requests[0].url.as_str().contains(secret),
                "non-query auth secret should not be in the URL"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn basic_auth_username_secret_absent_from_debug_output() -> Result<(), ApiClientError> {
        let (events, requests) =
            run_debug_request(auth_policy(AuthPlacement::Basic), StatusCode::OK).await?;

        let debug_output = events.join("\n");
        assert_secret_absent(&debug_output, BASIC_USERNAME_SECRET);
        assert_secret_absent(&debug_output, PASSWORD_SECRET);
        assert_secret_absent(&format!("{:?}", requests[0]), BASIC_USERNAME_SECRET);
        assert_secret_absent(&format!("{:?}", requests[0]), PASSWORD_SECRET);
        Ok(())
    }

    #[tokio::test]
    async fn basic_auth_username_secret_absent_from_cache_key() -> Result<(), ApiClientError> {
        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
        let cache = Arc::new(RecordingCache::default());
        let mut policy = auth_policy(AuthPlacement::Basic);
        policy.cache = concord_core::internal::CacheSetting::Config(
            concord_core::advanced::CacheConfig::new(),
        );
        let mut client =
            ApiClient::<RedactionCx, _>::with_transport((), redaction_auth_vars(), transport);
        client.configure(|cfg| {
            cfg.cache_store(cache.clone());
        });

        client
            .request(RedactionEndpoint { policy })
            .execute_decoded()
            .await?;

        let output = cache.observations().await.join("\n");
        assert_secret_absent(&output, BASIC_USERNAME_SECRET);
        assert_secret_absent(&output, PASSWORD_SECRET);
        assert!(output.contains("hash:"));
        assert!(!output.contains("user:"));
        Ok(())
    }

    #[tokio::test]
    async fn basic_auth_username_and_password_reach_transport_authorization_header()
    -> Result<(), ApiClientError> {
        let (_events, requests) =
            run_debug_request(auth_policy(AuthPlacement::Basic), StatusCode::OK).await?;
        let header = requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .expect("basic auth header materialized");
        let encoded = header
            .strip_prefix("Basic ")
            .expect("basic auth header uses Basic scheme");
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .expect("valid basic auth");
        assert_eq!(
            String::from_utf8(decoded).expect("utf8 basic auth"),
            format!("{BASIC_USERNAME_SECRET}:{PASSWORD_SECRET}")
        );
        Ok(())
    }

    #[tokio::test]
    async fn transport_error_hook_url_redacts_query_auth_secret() -> Result<(), ApiClientError> {
        let (output, requests) =
            run_transport_error_request(policy_with_query_auth("api_key")).await?;

        assert!(
            output.contains("transport_error:https://example.com/text?page=2&api_key=<redacted>")
        );
        assert_secret_absent(&output, API_KEY_SECRET);
        assert!(requests[0].url.as_str().contains(API_KEY_SECRET));
        Ok(())
    }

    #[tokio::test]
    async fn auth_errors_include_names_but_not_configured_secrets() {
        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
        let client = ApiClient::<RedactionCx, _>::with_transport(
            (),
            RedactionAuthVars {
                api_key: String::new(),
                bearer: String::new(),
                username: String::new(),
                password: String::new(),
            },
            transport,
        );

        let err = client
            .request(RedactionEndpoint {
                policy: auth_policy(AuthPlacement::Header("X-Api-Key")),
            })
            .execute_decoded()
            .await
            .expect_err("empty header secret should fail before transport");

        let output = format!("{err:?}\n{err}");
        assert!(output.contains("Redaction"));
        assert!(output.contains("test.token"));
        assert!(output.contains("test-token"));
        assert_secret_absent(&output, API_KEY_SECRET);
        assert_secret_absent(&output, BEARER_SECRET);
        assert_secret_absent(&output, PASSWORD_SECRET);
    }

    #[tokio::test]
    async fn auth_refresh_does_not_retain_old_or_new_raw_auth_in_debug_surfaces()
    -> Result<(), ApiClientError> {
        #[derive(Clone)]
        struct RefreshAuthVars {
            prepares: Arc<AtomicUsize>,
        }

        #[derive(Clone)]
        struct RefreshCx;

        impl ClientContext for RefreshCx {
            type Vars = ();
            type AuthVars = RefreshAuthVars;
            type AuthState = ();
            const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
            const DOMAIN: &'static str = "example.com";

            fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

            fn prepare_auth_requirement<'a>(
                requirement: &'a AuthRequirement,
                request: &'a mut AuthApplicationRequest<'_>,
                _vars: &'a Self::Vars,
                auth: &'a Self::AuthVars,
                _auth_state: &'a Self::AuthState,
                _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
                _meta: &'a RequestMeta,
            ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>>
            {
                Box::pin(async move {
                    let prepare_index = auth.prepares.fetch_add(1, Ordering::SeqCst);
                    let secret = if prepare_index == 0 {
                        REFRESH_SECRET_A
                    } else {
                        REFRESH_SECRET_B
                    };
                    let material = ApiKey::new(secret);
                    let application = apply_secret_credential(request, requirement, &material)?;
                    let applied = AuthAppliedCredential {
                        credential_id: requirement.credential.id.clone(),
                        usage_id: requirement.usage_id.clone(),
                        step_id: requirement.step_id,
                        generation: Some((prepare_index + 1) as u64),
                        identity: application.identity().clone(),
                        provenance: requirement.provenance.clone(),
                    };
                    Ok(PreparedAuthCredential::new(applied, application))
                })
            }

            fn handle_auth_response<'a>(
                requirement: &'a AuthRequirement,
                applied: &'a AuthAppliedCredential,
                _vars: &'a Self::Vars,
                _auth: &'a Self::AuthVars,
                _auth_state: &'a Self::AuthState,
                _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
                _meta: &'a RequestMeta,
                status: StatusCode,
                _headers: &'a HeaderMap,
            ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>>
            {
                Box::pin(async move {
                    if status == StatusCode::UNAUTHORIZED {
                        Ok(AuthDecision::RetryAfterRefresh {
                            credential: requirement.credential.clone(),
                            generation: applied.generation,
                            reason: AuthRetryReason::Unauthorized,
                        })
                    } else {
                        Ok(AuthDecision::Continue)
                    }
                })
            }
        }

        #[derive(Clone)]
        struct RefreshEndpoint;

        impl Endpoint<RefreshCx> for RefreshEndpoint {
            type Response = String;

            fn plan(
                &self,
                _ctx: &ClientPlanContext<'_, RefreshCx>,
            ) -> Result<RequestPlan, ApiClientError> {
                Ok(request_plan(
                    "RefreshRedaction",
                    Method::GET,
                    "/refresh",
                    auth_policy(AuthPlacement::Header("X-Custom")),
                    None,
                    decode_string,
                ))
            }
        }

        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(
            events,
            vec![
                MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
                MockResponse::text(StatusCode::OK, "ok"),
            ],
        );
        let sent = transport.clone();
        let mut client = ApiClient::<RefreshCx, _>::with_transport(
            (),
            RefreshAuthVars {
                prepares: Arc::new(AtomicUsize::new(0)),
            },
            transport,
        );
        let debug = Arc::new(UrlDebugSink::default());
        client.set_debug_sink(debug.clone());

        let value = client
            .request(RefreshEndpoint)
            .debug_level(DebugLevel::VV)
            .execute_decoded()
            .await?;
        assert_eq!(value.into_value(), "ok");

        let debug_output = debug.events().join("\n");
        assert_secret_absent(&debug_output, REFRESH_SECRET_A);
        assert_secret_absent(&debug_output, REFRESH_SECRET_B);

        let requests = sent.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].extensions.pending_auth_slots[0].generation,
            Some(1)
        );
        assert_eq!(
            requests[1].extensions.pending_auth_slots[0].generation,
            Some(2)
        );
        assert_eq!(
            requests[0]
                .headers
                .get("X-Custom")
                .and_then(|v| v.to_str().ok()),
            Some(REFRESH_SECRET_A)
        );
        assert_eq!(
            requests[1]
                .headers
                .get("X-Custom")
                .and_then(|v| v.to_str().ok()),
            Some(REFRESH_SECRET_B)
        );
        for request in &requests {
            let debug_output = format!("{request:?}");
            assert_secret_absent(&debug_output, REFRESH_SECRET_A);
            assert_secret_absent(&debug_output, REFRESH_SECRET_B);
        }

        Ok(())
    }

    #[tokio::test]
    async fn certificate_auth_material_reaches_transport_request_only() -> Result<(), ApiClientError>
    {
        #[derive(Clone)]
        struct CertificateAuthVars {
            identity_id: String,
        }

        #[derive(Clone)]
        struct CertificateCx;

        impl ClientContext for CertificateCx {
            type Vars = ();
            type AuthVars = CertificateAuthVars;
            type AuthState = ();
            const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
            const DOMAIN: &'static str = "example.com";

            fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

            fn prepare_auth_requirement<'a>(
                requirement: &'a AuthRequirement,
                request: &'a mut AuthApplicationRequest<'_>,
                _vars: &'a Self::Vars,
                auth: &'a Self::AuthVars,
                _auth_state: &'a Self::AuthState,
                _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
                _meta: &'a RequestMeta,
            ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>>
            {
                Box::pin(async move {
                    let material = ClientCertificate::new(auth.identity_id.clone());
                    let application =
                        apply_certificate_credential(request, requirement, &material)?;
                    let applied = AuthAppliedCredential {
                        credential_id: requirement.credential.id.clone(),
                        usage_id: requirement.usage_id.clone(),
                        step_id: requirement.step_id,
                        generation: Some(7),
                        identity: application.identity().clone(),
                        provenance: requirement.provenance.clone(),
                    };
                    Ok(PreparedAuthCredential::new(applied, application))
                })
            }
        }

        #[derive(Clone)]
        struct CertificateEndpoint;

        impl Endpoint<CertificateCx> for CertificateEndpoint {
            type Response = String;

            fn plan(
                &self,
                _ctx: &ClientPlanContext<'_, CertificateCx>,
            ) -> Result<RequestPlan, ApiClientError> {
                Ok(request_plan(
                    "CertificateRedaction",
                    Method::GET,
                    "/certificate",
                    auth_policy(AuthPlacement::Certificate),
                    None,
                    decode_string,
                ))
            }
        }

        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "ok")]);
        let sent = transport.clone();
        let client = ApiClient::<CertificateCx, _>::with_transport(
            (),
            CertificateAuthVars {
                identity_id: CERTIFICATE_ID.to_string(),
            },
            transport,
        );

        client
            .request(CertificateEndpoint)
            .execute_decoded()
            .await?
            .into_value();

        let requests = sent.requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].transport_auth,
            Some(TransportAuth::ClientCertificate {
                identity_id: CERTIFICATE_ID.to_string()
            })
        );
        assert_eq!(
            requests[0].extensions.pending_auth_slots[0].generation,
            Some(7)
        );
        assert_secret_absent(&format!("{:?}", requests[0]), CERTIFICATE_ID);

        Ok(())
    }

    #[tokio::test]
    async fn internal_auth_uses_sealed_request_and_materializes_only_at_transport()
    -> Result<(), ApiClientError> {
        #[derive(Clone)]
        struct InternalAuthVars {
            internal_secret: String,
            external_secret: String,
        }

        #[derive(Clone)]
        struct InternalAuthCx;

        impl ClientContext for InternalAuthCx {
            type Vars = ();
            type AuthVars = InternalAuthVars;
            type AuthState = ();
            const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
            const DOMAIN: &'static str = "example.com";

            fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

            fn apply_internal_auth<'a>(
                requirement: &'a AuthRequirementId,
                request: &'a mut AuthApplicationRequest<'_>,
                _vars: &'a Self::Vars,
                auth: &'a Self::AuthVars,
                _auth_state: &'a Self::AuthState,
                _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
            ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedInternalAuth, AuthError>>
            {
                Box::pin(async move {
                    assert_eq!(requirement.name(), "internal");
                    let requirement = AuthRequirement {
                        credential: concord_core::advanced::CredentialRef {
                            id: concord_core::advanced::CredentialId::new("test", "internal"),
                        },
                        placement: AuthPlacement::Header("X-Internal-Custom"),
                        usage_id: concord_core::advanced::AuthUsageId::new("internal-use"),
                        step_id: Some("internal"),
                        provenance: concord_core::advanced::AuthProvenance::new("internal"),
                        challenge: Default::default(),
                    };
                    let material = ApiKey::new(auth.internal_secret.clone());
                    let application = apply_secret_credential(request, &requirement, &material)?;
                    Ok(PreparedInternalAuth::from_application(application))
                })
            }

            fn prepare_auth_requirement<'a>(
                requirement: &'a AuthRequirement,
                request: &'a mut AuthApplicationRequest<'_>,
                _vars: &'a Self::Vars,
                auth: &'a Self::AuthVars,
                _auth_state: &'a Self::AuthState,
                executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
                _meta: &'a RequestMeta,
            ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>>
            {
                Box::pin(async move {
                    let auth_resp = executor
                        .send(AuthHttpRequest {
                            method: Method::GET,
                            url: "https://auth.example.com/internal"
                                .parse()
                                .expect("auth url"),
                            headers: HeaderMap::new(),
                            body: None,
                            mode: AuthMode::UseAuth(AuthRequirementId::new("test", "internal")),
                            policy: AuthInternalPolicy::default(),
                        })
                        .await?;
                    assert_eq!(auth_resp.status, StatusCode::OK);

                    let material = AccessToken::new(auth.external_secret.clone());
                    let application = apply_secret_credential(request, requirement, &material)?;
                    let applied = AuthAppliedCredential {
                        credential_id: requirement.credential.id.clone(),
                        usage_id: requirement.usage_id.clone(),
                        step_id: requirement.step_id,
                        generation: Some(1),
                        identity: application.identity().clone(),
                        provenance: requirement.provenance.clone(),
                    };
                    Ok(PreparedAuthCredential::new(applied, application))
                })
            }
        }

        struct InternalEndpoint;

        impl Endpoint<InternalAuthCx> for InternalEndpoint {
            type Response = String;

            fn plan(
                &self,
                _ctx: &ClientPlanContext<'_, InternalAuthCx>,
            ) -> Result<RequestPlan, ApiClientError> {
                Ok(request_plan(
                    "InternalAuth",
                    Method::GET,
                    "/protected",
                    auth_policy(AuthPlacement::Bearer),
                    None,
                    decode_string,
                ))
            }
        }

        let transport = MockTransport::new(
            Arc::new(TokioMutex::new(Vec::new())),
            vec![
                MockResponse::text(StatusCode::OK, "internal-ok"),
                MockResponse::text(StatusCode::OK, "protected-ok"),
            ],
        );
        let sent = transport.clone();
        let client = ApiClient::<InternalAuthCx, _>::with_transport(
            (),
            InternalAuthVars {
                internal_secret: INTERNAL_AUTH_SECRET.to_string(),
                external_secret: BEARER_SECRET.to_string(),
            },
            transport,
        );

        let value = client
            .request(InternalEndpoint)
            .execute_decoded()
            .await?
            .into_value();
        assert_eq!(value, "protected-ok");

        let requests = sent.requests().await;
        assert_eq!(requests.len(), 2);
        let internal_header = requests[0]
            .headers
            .get("X-Internal-Custom")
            .and_then(|value| value.to_str().ok())
            .expect("internal auth header materialized");
        assert_eq!(internal_header, INTERNAL_AUTH_SECRET);
        assert_secret_absent(&format!("{:?}", requests[0]), INTERNAL_AUTH_SECRET);
        assert_secret_absent(
            &format!("{:?}", requests[0].extensions),
            INTERNAL_AUTH_SECRET,
        );
        assert_secret_absent(&format!("{:?}", requests[1]), INTERNAL_AUTH_SECRET);
        assert_eq!(requests[0].extensions.pending_auth_slots.len(), 1);
        assert_eq!(
            requests[0].extensions.pending_auth_slots[0].generation,
            None
        );
        Ok(())
    }

    #[test]
    fn secret_wrappers_redact_debug_and_display() {
        for (debug_output, display_output, secret) in [
            {
                let value = concord_core::prelude::SecretString::new(API_KEY_SECRET);
                (format!("{value:?}"), format!("{value}"), API_KEY_SECRET)
            },
            {
                let value = ApiKey::new(API_KEY_SECRET);
                (
                    format!("{value:?}"),
                    format!("{}", value.value),
                    API_KEY_SECRET,
                )
            },
            {
                let value = AccessToken::new(BEARER_SECRET);
                (
                    format!("{value:?}"),
                    format!("{}", value.token),
                    BEARER_SECRET,
                )
            },
            {
                let value = BasicCredential::new(BASIC_USERNAME_SECRET, PASSWORD_SECRET);
                (
                    format!("{value:?}"),
                    format!("{}:{}", value.username, value.password),
                    BASIC_USERNAME_SECRET,
                )
            },
        ] {
            assert!(debug_output.contains("<secret>"));
            assert!(display_output.contains("<secret>"));
            assert_secret_absent(&debug_output, secret);
            assert_secret_absent(&display_output, secret);
        }
        let basic = BasicCredential::new(BASIC_USERNAME_SECRET, PASSWORD_SECRET);
        assert_secret_absent(&format!("{basic:?}"), PASSWORD_SECRET);
        assert_secret_absent(
            &format!("{}:{}", basic.username, basic.password),
            PASSWORD_SECRET,
        );
    }

    #[cfg(feature = "json")]
    #[tokio::test]
    async fn oauth2_client_credentials_errors_do_not_include_client_secret() {
        #[derive(Clone)]
        struct OAuthCx;

        impl ClientContext for OAuthCx {
            type Vars = ();
            type AuthVars = ();
            type AuthState = ();
            const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
            const DOMAIN: &'static str = "example.com";

            fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}
        }

        struct RejectingAuthExecutor;

        impl concord_core::advanced::AuthHttpExecutor for RejectingAuthExecutor {
            fn send<'a>(
                &'a self,
                request: AuthHttpRequest,
            ) -> concord_core::advanced::AuthFuture<
                'a,
                Result<concord_core::advanced::AuthHttpResponse, AuthError>,
            > {
                Box::pin(async move {
                    let debug_output = format!("{request:?}");
                    assert_secret_absent(&debug_output, CLIENT_SECRET);
                    assert!(debug_output.contains("<redacted>"));
                    assert!(debug_output.contains("body"));
                    let header = request
                        .headers
                        .get(http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default();
                    assert_secret_absent(header, CLIENT_SECRET);
                    let encoded = header
                        .strip_prefix("Basic ")
                        .expect("oauth2 client credentials should send basic auth");
                    let decoded =
                        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                            .expect("valid basic auth");
                    assert_eq!(
                        String::from_utf8(decoded).expect("utf8 basic auth"),
                        format!("visible-client-id:{CLIENT_SECRET}")
                    );
                    Ok(concord_core::advanced::AuthHttpResponse {
                        status: StatusCode::UNAUTHORIZED,
                        headers: HeaderMap::new(),
                        body: Bytes::from_static(b"{\"error\":\"invalid_client\"}"),
                    })
                })
            }
        }

        let provider = OAuth2ClientCredentialsProvider::new(
            concord_core::advanced::CredentialId::new("test", "oauth"),
            "https://auth.example.com/token".parse().expect("token url"),
            "visible-client-id",
            CLIENT_SECRET,
        );
        let ctx = concord_core::advanced::CredentialContext::<OAuthCx> {
            vars: &(),
            auth: &(),
            auth_state: &(),
            executor: &RejectingAuthExecutor,
            credential_id: concord_core::advanced::CredentialId::new("test", "oauth"),
            reason: concord_core::advanced::CredentialRefreshReason::Missing,
        };

        let err = provider
            .acquire(ctx)
            .await
            .expect_err("token endpoint rejection should be returned");
        let output = format!("{err:?}\n{err}");
        assert!(output.contains("oauth2 token endpoint returned 401"));
        assert_secret_absent(&output, CLIENT_SECRET);
    }
}
