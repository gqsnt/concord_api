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

                let meta = RequestMeta {
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
                let mut transport_retry_count: u32 = 0;
                loop {
                    let built = make_built_request(
                        &self.client.managed_client.client,
                        &base_request,
                        &mut body,
                    )?;

                    if policy.use_rate_limiter {
                        let _permit = self
                            .client
                            .runtime_state
                            .rate_limiter()
                            .acquire(RateLimitContext {
                                endpoint: "<auth>",
                                method: &built.context().meta.method,
                                url: &auth_url,
                                url_host: built.message.url().host_str(),
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
                    let resp = self
                        .client
                        .managed_client
                        .execute(native_request, Some(&context))
                        .await;
                    let mut resp = match resp {
                        Ok(resp) => resp,
                        Err(source) => {
                            if transport_retry_count >= policy.max_transport_retries {
                                return Err(AuthError::new(
                                    AuthErrorKind::AcquireFailed,
                                    source.to_string(),
                                ));
                            }
                            transport_retry_count =
                                next_auth_transport_retry_count(transport_retry_count)?;
                            continue;
                        }
                    };

                    let limit = policy.max_body_bytes as u64;
                    if let Some(actual) = resp.content_length()
                        && actual > limit
                    {
                        return Err(auth_body_limit_error(
                            crate::body::BodyError::limit_exceeded(limit, actual),
                        ));
                    }
                    let error_mapper = self.client.managed_client.response_error_mapper();
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

                    return Ok(AuthHttpResponse {
                        status: resp.status(),
                        headers: std::mem::take(resp.headers_mut()),
                        body: body.freeze(),
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

fn next_auth_transport_retry_count(retry_count: u32) -> Result<u32, AuthError> {
    retry_count.checked_add(1).ok_or_else(|| {
        AuthError::new(
            AuthErrorKind::AcquireFailed,
            "auth transport retry counter overflowed",
        )
    })
}
