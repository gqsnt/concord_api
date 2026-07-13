// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub(super) struct ClientAuthHttpExecutor<'a, Cx: ClientContext, T: Transport> {
    pub(super) client: &'a ApiClient<Cx, T>,
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
    if !body.supports_auth_internal_retries() {
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
    body.produce_for_attempt().map_err(|error| {
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

impl<Cx: ClientContext, T: Transport> AuthHttpExecutor for ClientAuthHttpExecutor<'_, Cx, T> {
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

                let meta = RequestMeta {
                    endpoint: "<auth>",
                    method,
                    idempotent: false,
                    attempt: 0,
                    page_index: 0,
                };

                let base_request = super::build::PublicRequestHead {
                    meta,
                    url,
                    headers,
                    timeout: policy.timeout,
                    retry: RetrySetting::Inherit,
                    rate_limit: RateLimitPlan::new(),
                    auth_plan,
                    reserved_headers: Vec::new(),
                };

                fn make_built_request(
                    base_request: &super::build::PublicRequestHead,
                    body: &mut crate::io::PreparedBody,
                    attempt: u32,
                ) -> Result<BuiltRequest, AuthError> {
                    let body = produce_auth_internal_body(body)?;
                    let ctx = ErrorContext {
                        endpoint: "<auth>",
                        method: base_request.meta.method.clone(),
                    };
                    let head = super::build::PublicRequestHead {
                        meta: RequestMeta {
                            attempt,
                            ..base_request.meta.clone()
                        },
                        url: base_request.url.clone(),
                        headers: base_request.headers.clone(),
                        timeout: base_request.timeout,
                        retry: RetrySetting::Inherit,
                        rate_limit: RateLimitPlan::new(),
                        auth_plan: base_request.auth_plan.clone(),
                        reserved_headers: base_request.reserved_headers.clone(),
                    };
                    head.finish(body, &ctx).map_err(|_| {
                        AuthError::new(
                            AuthErrorKind::InvalidConfiguration,
                            "auth-internal request URI is invalid",
                        )
                    })
                }

                let mut auth_materials = Vec::new();
                match mode {
                    AuthMode::SkipAuth => {}
                    AuthMode::UseAuth { id, requirement: _ } => {
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
                        let applied = {
                            let slot = base_request
                                .auth_plan
                                .slots
                                .first()
                                .expect("validated internal auth plan must contain one slot");
                            let mut auth_request = crate::auth::AuthApplicationRequest::new(slot);
                            Cx::apply_internal_auth(
                                &id,
                                &mut auth_request,
                                self.client.vars(),
                                self.client.auth_vars(),
                                auth_state_snapshot.as_ref(),
                                self,
                            )
                            .await
                        };
                        let applied = applied?;
                        applied.validate_bindings(&base_request.auth_plan)?;
                        auth_materials = applied.materials;
                    }
                }

                let auth_url = base_request.url.as_str().to_string();
                let mut attempt: u32 = 0;
                loop {
                    let built = make_built_request(&base_request, &mut body, attempt)?;

                    if policy.use_rate_limiter {
                        let _permit = self
                            .client
                            .runtime_state
                            .rate_limiter()
                            .acquire(RateLimitContext {
                                endpoint: "<auth>",
                                method: &built.context().meta.method,
                                url: &auth_url,
                                url_host: built.message.uri().host(),
                                attempt,
                                page_index: 0,
                                idempotent: built.context().meta.idempotent,
                                max_cooldown: self.client.runtime_state.max_rate_limit_cooldown(),
                                plan: &built.rate_limit,
                            })
                            .await
                            .map_err(|source| {
                                AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                            })?;
                    }

                    let transport_req = crate::transport::materialize_transport_request(
                        built,
                        &auth_materials,
                        None,
                    )
                    .map_err(|source| {
                        AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                    })?;
                    let resp = self.client.transport.send(transport_req).await;
                    let resp = match resp {
                        Ok(resp) => resp,
                        Err(source) => {
                            if attempt >= policy.max_transport_retries {
                                return Err(AuthError::new(
                                    AuthErrorKind::AcquireFailed,
                                    source.to_string(),
                                ));
                            }
                            attempt = next_auth_transport_attempt(attempt)?;
                            continue;
                        }
                    };

                    let (parts, response_body) = resp.into_parts();
                    let response_body = crate::body::limit_response_body(
                        response_body,
                        Some(policy.max_body_bytes),
                    )
                    .map_err(auth_body_limit_error)?;
                    let body = crate::body::collect_body(response_body)
                        .await
                        .map_err(|source| {
                            if source.kind() == crate::body::BodyErrorKind::LimitExceeded {
                                auth_body_limit_error(source)
                            } else {
                                AuthError::new(
                                    AuthErrorKind::ResponseBody,
                                    format!("auth response body read failed ({:?})", source.kind()),
                                )
                            }
                        })?
                        .to_bytes();

                    return Ok(AuthHttpResponse {
                        status: parts.status,
                        headers: parts.headers,
                        body,
                    });
                }
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

fn next_auth_transport_attempt(attempt: u32) -> Result<u32, AuthError> {
    attempt.checked_add(1).ok_or_else(|| {
        AuthError::new(
            AuthErrorKind::AcquireFailed,
            "auth transport attempt counter overflowed",
        )
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use bytes::Bytes;
    use futures_core::Stream;
    use http::HeaderValue;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};

    fn internal_requirement() -> crate::auth::AuthRequirement {
        crate::auth::AuthRequirement {
            credential: crate::auth::CredentialRef {
                id: crate::auth::CredentialId::new("test", "internal"),
            },
            placement: crate::auth::AuthPlacement::Header("X-Internal"),
            usage_id: crate::auth::AuthUsageId::new("internal-use"),
            step_id: Some("internal"),
            provenance: crate::auth::AuthProvenance::new("internal"),
            challenge: Default::default(),
        }
    }

    #[derive(Clone)]
    struct InternalPreflightCx;

    impl crate::client::ClientContext for InternalPreflightCx {
        type Vars = ();
        type AuthVars = Arc<AtomicUsize>;
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.com";

        fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

        fn apply_internal_auth<'a>(
            _id: &'a crate::auth::AuthRequirementId,
            request: &'a mut crate::auth::AuthApplicationRequest<'_>,
            _vars: &'a Self::Vars,
            calls: &'a Self::AuthVars,
            _auth_state: &'a Self::AuthState,
            _executor: &'a dyn crate::auth::AuthHttpExecutor,
        ) -> crate::auth::AuthFuture<
            'a,
            Result<crate::auth::PreparedInternalAuth, crate::auth::AuthError>,
        > {
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let requirement = internal_requirement();
                let material = crate::auth::ApiKey::new("internal-secret");
                let application =
                    crate::auth::apply_secret_credential(request, &requirement, &material)?;
                Ok(crate::auth::PreparedInternalAuth::from_application(
                    application,
                ))
            })
        }
    }

    #[derive(Clone)]
    struct CountingTransport(Arc<AtomicUsize>);

    impl crate::transport::Transport for CountingTransport {
        fn send(
            &self,
            _req: http::Request<crate::body::DynBody>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            http::Response<crate::body::DynBody>,
                            crate::transport::TransportError,
                        >,
                    > + Send,
            >,
        > {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Err(crate::transport::TransportError::with_kind(
                    crate::transport::TransportErrorKind::Other,
                    std::io::Error::other("unexpected transport invocation"),
                ))
            })
        }
    }

    #[derive(Clone)]
    struct ResponseBodyFailingTransport(Arc<AtomicUsize>);

    struct SingleErrorBody(bool);

    impl Stream for SingleErrorBody {
        type Item = Result<Bytes, crate::body::BodyError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if self.0 {
                self.0 = false;
                Poll::Ready(Some(Err(crate::body::BodyError::input())))
            } else {
                Poll::Ready(None)
            }
        }
    }

    impl crate::transport::Transport for ResponseBodyFailingTransport {
        fn send(
            &self,
            _req: http::Request<crate::body::DynBody>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            http::Response<crate::body::DynBody>,
                            crate::transport::TransportError,
                        >,
                    > + Send,
            >,
        > {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(http::Response::new(crate::body::DynBody::from_byte_stream(
                    SingleErrorBody(true),
                )))
            })
        }
    }
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn auth_attempt_counter_overflow_returns_error() {
        let err = next_auth_transport_attempt(u32::MAX)
            .expect_err("overflowing auth transport attempt counter should fail");
        assert_eq!(err.kind, AuthErrorKind::AcquireFailed);
        assert!(
            err.to_string()
                .contains("auth transport attempt counter overflowed")
        );
    }

    #[tokio::test]
    async fn reusable_auth_body_produces_fresh_attempts() {
        let mut body = crate::io::PreparedBody::reusable_bytes(
            Bytes::from_static(b"auth-body"),
            Some(HeaderValue::from_static(
                "application/x-www-form-urlencoded",
            )),
        );
        let mut headers = http::HeaderMap::new();

        validate_auth_internal_body(&mut headers, &body).expect("body should be supported");
        assert_eq!(
            headers.get(http::header::CONTENT_TYPE),
            Some(&HeaderValue::from_static(
                "application/x-www-form-urlencoded"
            ))
        );

        for _ in 0..2 {
            let produced = produce_auth_internal_body(&mut body).expect("attempt body");
            let bytes = http_body_util::BodyExt::collect(produced.into_dyn_body())
                .await
                .expect("body")
                .to_bytes();
            assert_eq!(bytes, Bytes::from_static(b"auth-body"));
        }
    }

    #[test]
    fn auth_rebuildability_is_recipe_derived_without_factory_invocation() {
        let invocations = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&invocations);
        let factory = crate::io::PreparedBody::replay_factory(
            http_body::SizeHint::default(),
            None,
            move || {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(crate::body::DynBody::empty())
            },
        );
        let mut headers = http::HeaderMap::new();

        validate_auth_internal_body(&mut headers, &factory)
            .expect("complete factories are rebuildable for auth recovery");
        assert_eq!(invocations.load(Ordering::SeqCst), 0);

        let one_shot = crate::io::PreparedBody::one_shot(crate::body::DynBody::empty(), None);
        validate_auth_internal_body(&mut headers, &one_shot)
            .expect_err("auth one-shot bodies are unsupported");
    }

    #[test]
    fn auth_body_media_type_conflicts_are_rejected_safely() {
        let body = crate::io::PreparedBody::reusable_bytes(
            Bytes::from_static(b"secret-auth-body"),
            Some(HeaderValue::from_static("application/json")),
        );
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );

        let error = validate_auth_internal_body(&mut headers, &body)
            .expect_err("conflicting body media type must fail");
        let rendered = format!("{error:?} {error}");
        assert!(rendered.contains("conflicts with prepared body media type"));
        assert!(!rendered.contains("secret-auth-body"));
        assert!(!rendered.contains("application/json"));
        assert!(!rendered.contains("text/plain"));
    }

    #[tokio::test]
    async fn internal_auth_collision_precedes_apply_internal_auth() {
        let calls = Arc::new(AtomicUsize::new(0));
        let sends = Arc::new(AtomicUsize::new(0));
        let client = ApiClient::<InternalPreflightCx, _>::with_transport(
            (),
            calls.clone(),
            CountingTransport(sends.clone()),
        );
        let executor = ClientAuthHttpExecutor { client: &client };
        let mut headers = http::HeaderMap::new();
        headers.insert("x-internal", HeaderValue::from_static("public"));

        let error = crate::auth::AuthHttpExecutor::send(
            &executor,
            crate::auth::AuthHttpRequest {
                method: http::Method::GET,
                url: "https://auth.example.com/token".parse().expect("url"),
                headers,
                body: crate::io::PreparedBody::empty(),
                mode: crate::auth::AuthMode::use_auth(
                    crate::auth::AuthRequirementId::new("test", "internal"),
                    internal_requirement(),
                ),
                policy: crate::auth::AuthInternalPolicy::default(),
            },
        )
        .await
        .expect_err("public internal-auth collision must fail");

        assert_eq!(error.kind, AuthErrorKind::InvalidConfiguration);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn auth_internal_response_body_error_is_terminal() {
        let sends = Arc::new(AtomicUsize::new(0));
        let client = ApiClient::<InternalPreflightCx, _>::with_transport(
            (),
            Arc::new(AtomicUsize::new(0)),
            ResponseBodyFailingTransport(sends.clone()),
        );
        let executor = ClientAuthHttpExecutor { client: &client };
        let error = crate::auth::AuthHttpExecutor::send(
            &executor,
            crate::auth::AuthHttpRequest {
                method: http::Method::POST,
                url: "https://auth.example.com/token".parse().expect("url"),
                headers: http::HeaderMap::new(),
                body: crate::io::PreparedBody::reusable_bytes(
                    Bytes::from_static(b"auth-body"),
                    None,
                ),
                mode: crate::auth::AuthMode::SkipAuth,
                policy: crate::auth::AuthInternalPolicy {
                    max_transport_retries: 2,
                    ..Default::default()
                },
            },
        )
        .await
        .expect_err("post-header auth body failure must not retry");

        assert_eq!(error.kind, AuthErrorKind::ResponseBody);
        assert_eq!(sends.load(Ordering::SeqCst), 1);
        assert!(!format!("{error:?} {error}").contains("auth-body"));
    }
}
