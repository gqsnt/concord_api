struct ClientAuthHttpExecutor<'a, Cx: ClientContext, T: Transport> {
    client: &'a ApiClient<Cx, T>,
}

tokio::task_local! {
    static AUTH_INTERNAL_STACK: RefCell<Vec<String>>;
}

async fn with_auth_internal_stack<T>(fut: impl std::future::Future<Output = T>) -> T {
    if AUTH_INTERNAL_STACK.try_with(|_| ()).is_ok() {
        fut.await
    } else {
        AUTH_INTERNAL_STACK
            .scope(RefCell::new(Vec::new()), fut)
            .await
    }
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
                    headers,
                    body,
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

                let mut base_request = BuiltRequest {
                    meta,
                    url,
                    headers,
                    body,
                    timeout: policy.timeout,
                    retry: RetrySetting::Inherit,
                    rate_limit: RateLimitPlan::new(),
                    extensions: Default::default(),
                };

                fn make_built_request(
                    base_request: &BuiltRequest,
                    attempt: u32,
                ) -> Result<BuiltRequest, AuthError> {
                    let body = match &base_request.body {
                        crate::transport::TransportRequestBody::Empty => {
                            crate::transport::TransportRequestBody::Empty
                        }
                        crate::transport::TransportRequestBody::Bytes(bytes) => {
                            crate::transport::TransportRequestBody::from_bytes(bytes.clone())
                        }
                        crate::transport::TransportRequestBody::Stream(_) => {
                            return Err(AuthError::new(
                                AuthErrorKind::UnsupportedScheme,
                                "stream auth request bodies are not supported yet",
                            ));
                        }
                    };
                    Ok(BuiltRequest {
                        meta: RequestMeta {
                            attempt,
                            ..base_request.meta.clone()
                        },
                        url: base_request.url.clone(),
                        headers: base_request.headers.clone(),
                        body,
                        timeout: base_request.timeout,
                        retry: base_request.retry.clone(),
                        rate_limit: base_request.rate_limit.clone(),
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

                        AUTH_INTERNAL_STACK.with(|stack| stack.borrow_mut().push(requirement_key));
                        let auth_state_snapshot = self.client.try_auth_state()?;
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
                        AUTH_INTERNAL_STACK.with(|stack| {
                            let _ = stack.borrow_mut().pop();
                        });
                        auth_materials = applied?.materials;
                    }
                }

                let auth_url = base_request.url.as_str().to_string();
                let mut attempt: u32 = 0;
                loop {
                    let built = make_built_request(&base_request, attempt)?;

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
                                plan: &built.rate_limit,
                            })
                            .await
                            .map_err(|source| {
                                AuthError::new(AuthErrorKind::AcquireFailed, source.to_string())
                            })?;
                    }

                    let transport_req =
                        crate::transport::materialize_transport_request(built, &auth_materials)
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

    #[test]
    fn auth_attempt_counter_overflow_returns_error() {
        let err = next_auth_transport_attempt(u32::MAX)
            .expect_err("overflowing auth transport attempt counter should fail");
        assert_eq!(err.kind, AuthErrorKind::AcquireFailed);
        assert!(err.to_string().contains("auth transport attempt counter overflowed"));
    }
}
