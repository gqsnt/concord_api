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
) -> Result<crate::io::AttemptBody, AuthError> {
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

                let meta = RequestMeta {
                    endpoint: "<auth>",
                    method,
                    idempotent: false,
                    attempt: 0,
                    page_index: 0,
                };

                validate_auth_internal_body(&mut headers, &body)?;
                let mut base_request = BuiltRequest {
                    meta,
                    url,
                    headers,
                    body: crate::io::AttemptBody::Empty,
                    timeout: policy.timeout,
                    retry: RetrySetting::Inherit,
                    rate_limit: RateLimitPlan::new(),
                    extensions: Default::default(),
                };

                fn make_built_request(
                    base_request: &BuiltRequest,
                    body: &mut crate::io::PreparedBody,
                    attempt: u32,
                ) -> Result<BuiltRequest, AuthError> {
                    let body = produce_auth_internal_body(body)?;
                    Ok(BuiltRequest {
                        meta: RequestMeta {
                            attempt,
                            ..base_request.meta.clone()
                        },
                        url: base_request.url.clone(),
                        headers: base_request.headers.clone(),
                        body,
                        timeout: base_request.timeout,
                        retry: RetrySetting::Inherit,
                        rate_limit: RateLimitPlan::new(),
                        extensions: base_request.extensions.clone(),
                    })
                }

                let mut auth_materials = Vec::new();
                match mode {
                    AuthMode::SkipAuth => {}
                    AuthMode::UseAuth(requirement) => {
                        let requirement_key = requirement.safe_fragment();
                        let recursive = AUTH_INTERNAL_STACK.with(|stack| {
                            stack.borrow().iter().any(|item| item == &requirement_key)
                        });
                        if recursive {
                            return Err(AuthError::new(
                                AuthErrorKind::RecursionDetected,
                                format!(
                                "internal auth recursion detected for requirement `{requirement}`"
                            ),
                        ));
                    }

                        let auth_state_snapshot = self.client.try_auth_state()?;
                        let _stack_guard = AuthInternalStackGuard::push(requirement_key);
                        let applied = {
                            let mut auth_request =
                                crate::auth::AuthApplicationRequest::new(&mut base_request.extensions);
                            Cx::apply_internal_auth(
                                &requirement,
                                &mut auth_request,
                                self.client.vars(),
                                self.client.auth_vars(),
                                auth_state_snapshot.as_ref(),
                                self,
                            )
                            .await
                        };
                        auth_materials = applied?.materials;
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
                                method: &built.meta.method,
                                url: &auth_url,
                                url_host: built.url.host_str(),
                                attempt,
                                page_index: 0,
                                idempotent: built.meta.idempotent,
                                max_cooldown: self.client.runtime_state.max_rate_limit_cooldown(),
                                plan: &built.rate_limit,
                            })
                            .await
                            .map_err(|source| {
                                AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                            })?;
                    }

                    let transport_req =
                        crate::transport::materialize_transport_request(
                            built,
                            &auth_materials,
                            None,
                        )
                        .map_err(|source| {
                            AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                        })?;
                    let resp = self.client.transport.send(transport_req).await;
                    let mut resp = match resp {
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

                    let body = match read_body_all_limited(
                        resp.body.as_mut(),
                        resp.content_length,
                        Some(policy.max_body_bytes),
                    )
                    .await
                    {
                        Ok(body) => body,
                        Err(BodyReadError::Transport(source)) => {
                            if attempt >= policy.max_transport_retries {
                                return Err(AuthError::new(
                                    AuthErrorKind::AcquireFailed,
                                    source.to_string(),
                                ));
                            }
                            attempt = next_auth_transport_attempt(attempt)?;
                            continue;
                        }
                        Err(source @ BodyReadError::ContentLengthTooLarge { .. })
                        | Err(source @ BodyReadError::LimitExceeded { .. }) => {
                            return Err(auth_body_too_large_error(source));
                        }
                    };

                    return Ok(AuthHttpResponse {
                        status: resp.status,
                        headers: resp.headers,
                        body,
                    });
                }
            })
            .await
        })
    }
}

fn auth_body_too_large_error(error: BodyReadError) -> AuthError {
    AuthError::new(AuthErrorKind::ResponseTooLarge, error.to_string())
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
    use http::HeaderValue;
    use std::sync::Arc;
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

    #[test]
    fn reusable_auth_body_produces_fresh_attempts() {
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
            match produce_auth_internal_body(&mut body).expect("attempt body") {
                crate::io::AttemptBody::Bytes(bytes) => {
                    assert_eq!(bytes, Bytes::from_static(b"auth-body"));
                }
                _ => panic!("reusable auth bytes must remain buffered attempt bytes"),
            }
        }
    }

    #[test]
    fn unsupported_auth_bodies_are_rejected_without_production() {
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

        let error = validate_auth_internal_body(&mut headers, &factory)
            .expect_err("auth replay factories are unsupported");
        assert_eq!(error.kind, AuthErrorKind::UnsupportedScheme);
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
}
