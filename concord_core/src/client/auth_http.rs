// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub(super) struct ClientAuthHttpExecutor<'a, Cx: ClientContext> {
    pub(super) client: &'a ApiClient<Cx>,
}

tokio::task_local! {
    static AUTH_INTERNAL_STACK: std::cell::RefCell<Vec<String>>;
}

async fn with_auth_internal_stack<T>(fut: impl std::future::Future<Output = T>) -> T {
    if AUTH_INTERNAL_STACK.try_with(|_| ()).is_ok() {
        fut.await
    } else {
        AUTH_INTERNAL_STACK
            .scope(std::cell::RefCell::new(Vec::new()), fut)
            .await
    }
}

struct AuthInternalStackGuard {
    requirement_key: String,
}

impl AuthInternalStackGuard {
    fn push(requirement_key: String) -> Self {
        AUTH_INTERNAL_STACK.with(|stack| {
            stack.borrow_mut().push(requirement_key.clone());
        });
        Self { requirement_key }
    }
}

impl Drop for AuthInternalStackGuard {
    fn drop(&mut self) {
        let _ = AUTH_INTERNAL_STACK.try_with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack
                .last()
                .is_some_and(|item| item == &self.requirement_key)
            {
                stack.pop();
                return;
            }

            if let Some(index) = stack.iter().rposition(|item| item == &self.requirement_key) {
                stack.remove(index);
            }
        });
    }
}

fn validate_auth_internal_body(
    headers: &mut http::HeaderMap,
    body: &crate::io::PreparedBody,
) -> Result<(), AuthError> {
    if !body.is_replayable() {
        return Err(AuthError::new(
            AuthErrorKind::UnsupportedScheme,
            "auth-internal request body category is not supported",
        ));
    }
    crate::io::apply_prepared_body_media_type(headers, body).map_err(|()| {
        AuthError::new(
            AuthErrorKind::InvalidConfiguration,
            "auth-internal Content-Type conflicts with prepared body media type",
        )
    })
}

fn produce_auth_internal_body(
    body: &mut crate::io::PreparedBody,
) -> Result<crate::io::ProducedBody, AuthError> {
    body.produce_for_execution().map_err(|error| {
        let message = match error.kind() {
            crate::io::BodyProductionErrorKind::AlreadyConsumed => {
                "auth-internal one-shot request body was already consumed"
            }
            crate::io::BodyProductionErrorKind::FactoryFailure => {
                "auth-internal request body factory failed"
            }
        };
        AuthError::new(AuthErrorKind::AcquireFailed, message)
    })
}

impl<Cx: ClientContext> AuthHttpExecutor for ClientAuthHttpExecutor<'_, Cx> {
    fn send<'a>(
        &'a self,
        req: AuthHttpRequest,
    ) -> crate::auth::AuthFuture<'a, Result<AuthHttpResponse, AuthError>> {
        Box::pin(async move {
            with_auth_internal_stack(async move {
                let AuthHttpRequest {
                    method,
                    url,
                    mut headers,
                    mut body,
                    mode,
                    policy,
                } = req;

                validate_auth_internal_body(&mut headers, &body)?;
                let auth_plan = match &mode {
                    AuthMode::SkipAuth => crate::auth::AuthPlacementPlan::default(),
                    AuthMode::UseAuth { requirement, .. } => {
                        crate::auth::AuthPlacementPlan::from_auth_plan(&crate::auth::AuthPlan {
                            requirements: vec![requirement.clone()],
                        })?
                    }
                };
                auth_plan.validate_public_request(&headers, &url)?;

                let meta = RequestExecutionMeta {
                    endpoint: "<auth>",
                    method,
                    idempotent: false,
                    page_index: 0,
                };

                let base_request = super::build::PublicRequestHead {
                    meta,
                    url,
                    headers,
                    timeout: policy.timeout,
                    rate_limit: RateLimitPlan::new(),
                    auth_plan,
                    reserved_headers: Vec::new(),
                };

                fn make_built_request(
                    client: &reqwest::Client,
                    base_request: &super::build::PublicRequestHead,
                    body: &mut crate::io::PreparedBody,
                ) -> Result<BuiltRequest, AuthError> {
                    let body = produce_auth_internal_body(body)?;
                    let ctx = ErrorContext {
                        endpoint: "<auth>",
                        method: base_request.meta.method.clone(),
                    };
                    let head = super::build::PublicRequestHead {
                        meta: base_request.meta.clone(),
                        url: base_request.url.clone(),
                        headers: base_request.headers.clone(),
                        timeout: base_request.timeout,
                        rate_limit: RateLimitPlan::new(),
                        auth_plan: base_request.auth_plan.clone(),
                        reserved_headers: base_request.reserved_headers.clone(),
                    };
                    head.finish(client, body, &ctx).map_err(|_| {
                        AuthError::new(
                            AuthErrorKind::InvalidConfiguration,
                            "auth-internal request URI is invalid",
                        )
                    })
                }

                let mut auth_materials = Vec::new();
                match mode {
                    AuthMode::SkipAuth => {}
                    AuthMode::UseAuth { id, requirement } => {
                        let requirement_key = id.safe_fragment();
                        let recursive = AUTH_INTERNAL_STACK.with(|stack| {
                            stack.borrow().iter().any(|item| item == &requirement_key)
                        });
                        if recursive {
                            return Err(AuthError::new(
                                AuthErrorKind::RecursionDetected,
                                format!("internal auth recursion detected for requirement `{id}`"),
                            ));
                        }

                        let auth_state_snapshot = self.client.try_auth_state()?;
                        let _stack_guard = AuthInternalStackGuard::push(requirement_key);
                        let prepared = {
                            let slot = base_request
                                .auth_plan
                                .slots
                                .first()
                                .expect("validated internal auth plan must contain one slot");
                            let mut auth_request = crate::auth::AuthApplicationRequest::new(slot);
                            crate::auth::prepare::<Cx>(
                                &requirement,
                                &mut auth_request,
                                self.client.vars(),
                                self.client.auth_vars(),
                                auth_state_snapshot.as_ref(),
                                self,
                                &base_request.meta,
                            )
                            .await
                        };
                        let prepared = prepared?;
                        let slot = base_request
                            .auth_plan
                            .slots
                            .first()
                            .expect("validated internal auth plan must contain one slot");
                        prepared.validate_binding(slot)?;
                        auth_materials.push(prepared.material);
                    }
                }

                let provider_client = self.client.managed_client.provider();
                let built = make_built_request(&provider_client.client, &base_request, &mut body)?;

                let BuiltRequest {
                    message,
                    context,
                    auth_plan,
                    rate_limit: _,
                } = built;
                let native_request = crate::transport::materialize_authentication(
                    message,
                    &auth_plan,
                    &auth_materials,
                )
                .map_err(|source| {
                    AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                })?;
                // One Concord submission. Any approved protocol recovery is
                // internal to this separately managed provider Reqwest client.
                let mut resp = provider_client
                    .execute(native_request, Some(&context))
                    .await
                    .map_err(|source| {
                        AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                    })?;

                let limit = policy.max_body_bytes as u64;
                if let Some(actual) = resp.content_length()
                    && actual > limit
                {
                    return Err(auth_body_limit_error(
                        crate::body::BodyError::limit_exceeded(limit, actual),
                    ));
                }
                let error_mapper = provider_client.response_error_mapper();
                let mut body = bytes::BytesMut::new();
                let mut seen = 0_u64;
                loop {
                    let chunk = resp.chunk().await.map_err(|error| {
                        let source = error_mapper.map_body_error(error);
                        AuthError::new(
                            AuthErrorKind::ResponseBody,
                            format!("auth response body read failed ({:?})", source.kind()),
                        )
                    })?;
                    let Some(chunk) = chunk else {
                        break;
                    };
                    let actual = seen.saturating_add(chunk.len() as u64);
                    if actual > limit {
                        return Err(auth_body_limit_error(
                            crate::body::BodyError::limit_exceeded(limit, actual),
                        ));
                    }
                    body.extend_from_slice(&chunk);
                    seen = actual;
                }

                Ok(AuthHttpResponse {
                    status: resp.status(),
                    headers: std::mem::take(resp.headers_mut()),
                    body: body.freeze(),
                })
            })
            .await
        })
    }
}

fn auth_body_limit_error(error: crate::body::BodyError) -> AuthError {
    AuthError::new(
        AuthErrorKind::ResponseTooLarge,
        format!(
            "auth response body exceeded configured limit {} bytes",
            error.limit().unwrap_or_default()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthInternalPolicy;
    use crate::rate_limit::{
        RateLimitFuture, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext,
        RateLimiter,
    };
    use crate::runtime_hooks::{
        PostResponseHookContext, PreSendHookContext, RequestErrorHookContext, RuntimeHooks,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use crate::regression_tests::native_mock;

    #[derive(Clone)]
    struct ProviderHttpTestCx;

    impl ClientContext for ProviderHttpTestCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = ();

        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
        const DOMAIN: &'static str = "provider-http.example";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}
    }

    fn provider_request(url: url::Url, policy: AuthInternalPolicy) -> AuthHttpRequest {
        AuthHttpRequest {
            method: http::Method::POST,
            url,
            headers: http::HeaderMap::new(),
            body: crate::io::PreparedBody::empty(),
            mode: AuthMode::SkipAuth,
            policy,
        }
    }

    async fn send_provider(
        client: &ApiClient<ProviderHttpTestCx>,
        request: AuthHttpRequest,
    ) -> Result<AuthHttpResponse, AuthError> {
        ClientAuthHttpExecutor { client }.send(request).await
    }

    fn fixed_loopback_origin(
        server: &native_mock::MockServer,
    ) -> crate::retry_mode::ApiOriginDescriptor {
        let authority = server.base_url().authority().to_string().leak();
        crate::retry_mode::ApiOriginDescriptor::FixedSingleOrigin(
            crate::retry_mode::FixedOriginDescriptor {
                scheme: crate::retry_mode::OriginScheme::Http,
                authority,
            },
        )
    }

    #[tokio::test]
    async fn provider_protocol_recovery_submits_once_and_returns_status_for_classification() {
        let (server, handle) = native_mock::mock()
            .repeating(
                native_mock::MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE)
                    .with_body(bytes::Bytes::from_static(b"provider unavailable")),
            )
            .build();
        let client = ApiClient::<ProviderHttpTestCx>::new((), ());
        let response = send_provider(
            &client,
            provider_request(server.base_url().clone(), AuthInternalPolicy::default()),
        )
        .await
        .expect("status responses remain available to provider logic");

        assert_eq!(response.status, http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.body, "provider unavailable");
        assert_eq!(handle.wire_request_count(), 1);
    }

    #[tokio::test]
    async fn provider_disabled_submits_once_without_concord_resend() {
        const URL_SECRET: &str = "PROVIDER_REQUEST_URL_SECRET";
        let (server, handle) = native_mock::mock()
            .repeating(native_mock::MockReply::disconnect_after_request())
            .build();
        let client =
            ApiClient::<ProviderHttpTestCx>::with_safe_reqwest_builder((), (), |builder| {
                builder.provider_operation_retry_mode(
                    crate::retry_mode::ProviderOperationRetryMode::Disabled,
                )
            })
            .expect("disabled provider retry client");
        let mut url = server.base_url().clone();
        url.query_pairs_mut().append_pair("credential", URL_SECRET);
        let error = send_provider(
            &client,
            provider_request(url, AuthInternalPolicy::default()),
        )
        .await
        .expect_err("disconnect remains a provider request failure");

        assert_eq!(error.kind, AuthErrorKind::AcquireFailed);
        let rendered = format!("{error:?}\n{error}");
        assert!(!rendered.contains(URL_SECRET), "{rendered}");
        assert!(!rendered.contains(server.base_url().as_str()), "{rendered}");
        assert_eq!(handle.wire_request_count(), 1);
    }

    #[tokio::test]
    async fn application_status_retry_does_not_govern_provider_http() {
        let (server, handle) = native_mock::mock()
            .repeating(native_mock::MockReply::status(
                http::StatusCode::SERVICE_UNAVAILABLE,
            ))
            .build();
        let retry_mode =
            crate::retry_mode::RetryMode::status(2, [http::StatusCode::SERVICE_UNAVAILABLE])
                .expect("valid application status mode");
        let client = ApiClient::<ProviderHttpTestCx>::with_generated_descriptor_retry_mode(
            Some(fixed_loopback_origin(&server)),
            (),
            (),
            retry_mode,
            Ok,
        )
        .expect("application status-retry client");

        let response = send_provider(
            &client,
            provider_request(server.base_url().clone(), AuthInternalPolicy::default()),
        )
        .await
        .expect("provider status remains a response");

        assert_eq!(response.status, http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(handle.wire_request_count(), 1);
    }

    #[derive(Default)]
    struct CountingHooks {
        calls: AtomicUsize,
    }

    impl RuntimeHooks for CountingHooks {
        fn pre_send<'a>(
            &'a self,
            _ctx: PreSendHookContext<'a>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), ApiClientError>> + Send + 'a>,
        > {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }

        fn post_response<'a>(
            &'a self,
            _ctx: PostResponseHookContext<'a>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {})
        }

        fn request_error<'a>(
            &'a self,
            _ctx: RequestErrorHookContext<'a>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {})
        }
    }

    #[derive(Default)]
    struct CountingRateLimiter {
        acquisitions: AtomicUsize,
        responses: AtomicUsize,
    }

    impl RateLimiter for CountingRateLimiter {
        fn acquire<'a>(
            &'a self,
            _ctx: RateLimitContext<'a>,
        ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
            self.acquisitions.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(RateLimitPermit) })
        }

        fn on_response<'a>(
            &'a self,
            _ctx: RateLimitResponseContext<'a>,
        ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
            self.responses.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(RateLimitResponseAction::Continue) })
        }
    }

    #[tokio::test]
    async fn provider_http_bypasses_application_hooks_and_rate_limiter() {
        let (server, _handle) = native_mock::mock()
            .reply(native_mock::MockReply::status(http::StatusCode::OK))
            .build();
        let hooks = Arc::new(CountingHooks::default());
        let limiter = Arc::new(CountingRateLimiter::default());
        let mut client = ApiClient::<ProviderHttpTestCx>::new((), ());
        client.set_runtime_hooks(hooks.clone());
        client.set_rate_limiter(limiter.clone());

        send_provider(
            &client,
            provider_request(server.base_url().clone(), AuthInternalPolicy::default()),
        )
        .await
        .expect("provider request succeeds independently");

        assert_eq!(hooks.calls.load(Ordering::SeqCst), 0);
        assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
        assert_eq!(limiter.responses.load(Ordering::SeqCst), 0);
    }

    #[cfg(feature = "dangerous-dev-tools")]
    #[tokio::test]
    async fn deterministic_provider_and_application_executor_scripts_are_isolated() {
        use crate::__development::{
            DeterministicExecutionKind, DeterministicNativeExecutor, ScriptedNativeResponse,
            install_application_executor, install_provider_executor,
        };

        let application = DeterministicNativeExecutor::application();
        application.script_response(
            ScriptedNativeResponse::bytes(
                http::StatusCode::OK,
                bytes::Bytes::from_static(b"application"),
            )
            .with_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/plain"),
            ),
        );
        let provider = DeterministicNativeExecutor::provider();
        provider.script_response(ScriptedNativeResponse::bytes(
            http::StatusCode::CREATED,
            bytes::Bytes::from_static(b"provider"),
        ));

        let mut client = ApiClient::<ProviderHttpTestCx>::new((), ());
        install_application_executor(&mut client, application.clone())
            .expect("application installation");
        install_provider_executor(&mut client, provider.clone()).expect("provider installation");

        let provider_response = send_provider(
            &client,
            provider_request(
                "http://provider-http.example/token"
                    .parse()
                    .expect("provider URL"),
                AuthInternalPolicy::default(),
            ),
        )
        .await
        .expect("provider synthetic response");
        assert_eq!(provider_response.status, http::StatusCode::CREATED);
        assert_eq!(provider_response.body, "provider");
        assert_eq!(provider.captures().len(), 1);
        assert_eq!(
            provider.captures()[0].execution_kind(),
            DeterministicExecutionKind::Provider
        );
        assert_eq!(application.captures().len(), 0);
        assert_eq!(application.remaining_scripts(), 1);

        let application_response = client
            .execute_plan::<crate::prelude::Text<String>>(crate::regression_tests::request_plan(
                "DeterministicApplicationAfterProvider",
                http::Method::GET,
                "/application",
                Default::default(),
                None,
            ))
            .await
            .expect("application synthetic response");
        assert_eq!(application_response.value(), "application");
        assert_eq!(application.captures().len(), 1);
        assert_eq!(
            application.captures()[0].execution_kind(),
            DeterministicExecutionKind::Application
        );
        assert_eq!(provider.captures().len(), 1);
        assert_eq!(provider.remaining_scripts(), 0);
    }

    #[tokio::test]
    async fn provider_timeout_is_enforced_and_sanitized() {
        const URL_SECRET: &str = "PROVIDER_TIMEOUT_URL_SECRET";
        let (server, _handle) = native_mock::mock()
            .reply(
                native_mock::MockReply::status(http::StatusCode::OK)
                    .with_delay(Duration::from_millis(100)),
            )
            .build();
        let mut url = server.base_url().clone();
        url.query_pairs_mut().append_pair("credential", URL_SECRET);
        let client = ApiClient::<ProviderHttpTestCx>::new((), ());
        let error = send_provider(
            &client,
            provider_request(
                url,
                AuthInternalPolicy {
                    timeout: Some(Duration::from_millis(10)),
                    ..AuthInternalPolicy::default()
                },
            ),
        )
        .await
        .expect_err("provider timeout must be enforced");

        assert_eq!(error.kind, AuthErrorKind::AcquireFailed);
        let rendered = format!("{error:?}\n{error}");
        assert!(!rendered.contains(URL_SECRET), "{rendered}");
        assert!(!rendered.contains(server.base_url().as_str()), "{rendered}");
    }

    #[tokio::test]
    async fn provider_response_body_limit_is_enforced_separately_from_timeout() {
        let (server, _handle) = native_mock::mock()
            .reply(
                native_mock::MockReply::status(http::StatusCode::OK)
                    .with_body(bytes::Bytes::from_static(b"12345")),
            )
            .build();
        let client = ApiClient::<ProviderHttpTestCx>::new((), ());
        let error = send_provider(
            &client,
            provider_request(
                server.base_url().clone(),
                AuthInternalPolicy {
                    max_body_bytes: 4,
                    ..AuthInternalPolicy::default()
                },
            ),
        )
        .await
        .expect_err("provider response limit must be enforced");

        assert_eq!(error.kind, AuthErrorKind::ResponseTooLarge);
        assert_eq!(
            error.message,
            "auth response body exceeded configured limit 4 bytes"
        );
    }

    #[derive(Clone)]
    struct RecursiveAuthVars {
        provider_url: url::Url,
    }

    #[derive(Clone)]
    struct RecursiveProvider {
        provider_url: url::Url,
    }

    struct RecursiveAuthState {
        provider: Arc<crate::auth::CredentialProviderState<RecursiveCx, RecursiveProvider>>,
    }

    impl Clone for RecursiveAuthState {
        fn clone(&self) -> Self {
            Self {
                provider: self.provider.clone(),
            }
        }
    }

    #[derive(Clone)]
    struct RecursiveCx;

    fn recursive_requirement() -> crate::auth::AuthRequirement {
        crate::auth::AuthRequirement {
            credential: crate::auth::CredentialRef {
                id: crate::auth::CredentialId::new("provider-test", "recursive"),
            },
            placement: crate::auth::AuthPlacement::Bearer,
            usage_id: crate::auth::AuthUsageId::new("provider-recursion"),
            step_id: Some("recursive-provider"),
            provenance: crate::auth::AuthProvenance::new("provider-test"),
            challenge: Default::default(),
        }
    }

    impl ClientContext for RecursiveCx {
        type Vars = ();
        type AuthVars = RecursiveAuthVars;
        type AuthState = RecursiveAuthState;

        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
        const DOMAIN: &'static str = "provider-recursion.example";

        fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
            Self::AuthState {
                provider: Arc::new(crate::auth::CredentialProviderState::new(
                    RecursiveProvider {
                        provider_url: auth.provider_url.clone(),
                    },
                )),
            }
        }

        fn auth_provider_binding<'a>(
            credential: &crate::auth::CredentialId,
            auth_state: &'a Self::AuthState,
        ) -> Option<crate::auth::AuthProviderBinding<'a, Self>> {
            (credential == &crate::auth::CredentialId::new("provider-test", "recursive")).then(
                || {
                    auth_state.provider.secret_binding(
                        crate::auth::AuthPreparationMode::PerExecution,
                        crate::auth::AuthChallengeMode::InvalidateOnly,
                    )
                },
            )
        }
    }

    impl crate::auth::CredentialProvider<RecursiveCx> for RecursiveProvider {
        type Credential = crate::auth::ApiKey;

        fn id(&self) -> crate::auth::CredentialId {
            crate::auth::CredentialId::new("provider-test", "recursive")
        }

        fn acquire<'a>(
            &'a self,
            ctx: crate::auth::CredentialContext<'a, RecursiveCx>,
        ) -> crate::auth::AuthFuture<'a, Result<Self::Credential, AuthError>> {
            Box::pin(async move {
                ctx.executor
                    .send(AuthHttpRequest {
                        method: http::Method::POST,
                        url: self.provider_url.clone(),
                        headers: http::HeaderMap::new(),
                        body: crate::io::PreparedBody::empty(),
                        mode: AuthMode::use_auth(
                            crate::auth::AuthRequirementId::new(
                                "provider-test",
                                "recursive-operation",
                            ),
                            recursive_requirement(),
                        ),
                        policy: AuthInternalPolicy::default(),
                    })
                    .await?;
                Ok(crate::auth::ApiKey::new("unreachable"))
            })
        }
    }

    #[tokio::test]
    async fn provider_recursion_is_rejected_before_network_io() {
        let (server, handle) = native_mock::mock()
            .repeating(native_mock::MockReply::status(http::StatusCode::OK))
            .build();
        let client = ApiClient::<RecursiveCx>::new(
            (),
            RecursiveAuthVars {
                provider_url: server.base_url().clone(),
            },
        );
        let error = ClientAuthHttpExecutor { client: &client }
            .send(AuthHttpRequest {
                method: http::Method::POST,
                url: server.base_url().clone(),
                headers: http::HeaderMap::new(),
                body: crate::io::PreparedBody::empty(),
                mode: AuthMode::use_auth(
                    crate::auth::AuthRequirementId::new("provider-test", "recursive-operation"),
                    recursive_requirement(),
                ),
                policy: AuthInternalPolicy::default(),
            })
            .await
            .expect_err("recursive provider authentication must fail");

        assert_eq!(error.kind, AuthErrorKind::RecursionDetected);
        assert_eq!(handle.wire_request_count(), 0);
    }
}
