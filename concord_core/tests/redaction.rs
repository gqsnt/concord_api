#[allow(dead_code)]
#[path = "current_core/common.rs"]
mod common;

mod query_auth_redaction {
    use super::common::{MockResponse, MockTransport, auth_policy, decode_string, request_plan};
    use bytes::Bytes;
    use concord_core::advanced::{
        AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind, AuthPlacement,
        AuthRequirement, BuiltRequest, DebugSink, RequestMeta, RuntimeHooks, Transport,
        TransportError, TransportErrorHookContext, TransportResponse, apply_basic_credential,
        apply_secret_credential,
    };
    #[cfg(feature = "json")]
    use concord_core::advanced::{
        AuthHttpRequest, CredentialProvider, OAuth2ClientCredentialsProvider,
    };
    use concord_core::internal::{ClientPlanContext, RequestPlan, ResolvedPolicy};
    use concord_core::prelude::{
        AccessToken, ApiClient, ApiClientError, ApiKey, BasicCredential, ClientContext, DebugLevel,
        Endpoint,
    };
    use http::{HeaderMap, Method, StatusCode};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use tokio::sync::Mutex as TokioMutex;

    const API_KEY_SECRET: &str = "LEAK_SENTINEL_API_KEY_123";
    const BEARER_SECRET: &str = "LEAK_SENTINEL_BEARER_456";
    const PASSWORD_SECRET: &str = "LEAK_SENTINEL_PASSWORD_789";
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
            request: &'a mut BuiltRequest,
            _vars: &'a Self::Vars,
            auth: &'a Self::AuthVars,
            _auth_state: &'a Self::AuthState,
            _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
            _meta: &'a RequestMeta,
        ) -> concord_core::advanced::AuthFuture<'a, Result<AuthAppliedCredential, AuthError>>
        {
            Box::pin(async move {
                let identity = match requirement.placement {
                    AuthPlacement::Basic => {
                        let material = BasicCredential::new("sentinel-user", auth.password.clone());
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
                Ok(AuthAppliedCredential {
                    credential_id: requirement.credential.id.clone(),
                    usage_id: requirement.usage_id.clone(),
                    step_id: requirement.step_id,
                    generation: Some(1),
                    identity,
                    provenance: requirement.provenance.clone(),
                })
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

        fn request_headers(&self, _dbg: DebugLevel, _headers: &HeaderMap) {}

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
            password: PASSWORD_SECRET.to_string(),
        }
    }

    async fn run_debug_request(
        policy: ResolvedPolicy,
        status: StatusCode,
    ) -> Result<(Vec<String>, Vec<BuiltRequest>), ApiClientError> {
        let events = Arc::new(TokioMutex::new(Vec::new()));
        let transport = MockTransport::new(events, vec![MockResponse::text(status, "ok")]);
        let sent = transport.clone();
        let mut client =
            ApiClient::<RedactionCx, _>::with_transport((), redaction_auth_vars(), transport);
        let debug = Arc::new(UrlDebugSink::default());
        client.set_debug_sink(debug.clone());

        let request = client
            .request(RedactionEndpoint { policy })
            .debug_level(DebugLevel::V)
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
        requests: Arc<TokioMutex<Vec<BuiltRequest>>>,
    }

    impl FailingTransport {
        fn new() -> Self {
            Self {
                requests: Arc::new(TokioMutex::new(Vec::new())),
            }
        }

        async fn requests(&self) -> Vec<BuiltRequest> {
            self.requests.lock().await.clone()
        }
    }

    impl Transport for FailingTransport {
        fn send(
            &self,
            req: BuiltRequest,
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

    async fn run_transport_error_request(
        policy: ResolvedPolicy,
    ) -> Result<(String, Vec<BuiltRequest>), ApiClientError> {
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
                let value = BasicCredential::new("sentinel-user", PASSWORD_SECRET);
                (
                    format!("{value:?}"),
                    format!("{}", value.password),
                    PASSWORD_SECRET,
                )
            },
        ] {
            assert!(debug_output.contains("<secret>"));
            assert!(display_output.contains("<secret>"));
            assert_secret_absent(&debug_output, secret);
            assert_secret_absent(&display_output, secret);
        }
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
                    let header = request
                        .headers
                        .get(http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default();
                    assert_secret_absent(header, CLIENT_SECRET);
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
