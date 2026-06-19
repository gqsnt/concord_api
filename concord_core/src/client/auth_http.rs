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
            with_auth_internal_stack(async {
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

                if matches!(&mode, AuthMode::SkipAuth) {
                    let auth_url = url.as_str().to_string();
                    let base_request = crate::transport::TransportRequest {
                        meta,
                        url,
                        headers,
                        body,
                        timeout: policy.timeout,
                        rate_limit: RateLimitPlan::new(),
                        transport_auth: None,
                        extensions: Default::default(),
                    };
                    let mut attempt: u32 = 0;
                    loop {
                        let mut built = base_request.clone();
                        built.meta.attempt = attempt;

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
                                    AuthError::new(
                                        AuthErrorKind::AcquireFailed,
                                        source.to_string(),
                                    )
                                })?;
                        }

                        let resp = self.client.transport.send(built).await;
                        let mut resp = match resp {
                            Ok(resp) => resp,
                            Err(source) => {
                                if attempt >= policy.max_transport_retries {
                                    return Err(AuthError::new(
                                        AuthErrorKind::AcquireFailed,
                                        source.to_string(),
                                    ));
                                }
                                attempt = attempt.saturating_add(1);
                                continue;
                            }
                        };
                        let body = match read_body_all(resp.body.as_mut(), resp.content_length).await
                        {
                            Ok(body) => body,
                            Err(source) => {
                                if attempt >= policy.max_transport_retries {
                                    return Err(AuthError::new(
                                        AuthErrorKind::AcquireFailed,
                                        source.to_string(),
                                    ));
                                }
                                attempt = attempt.saturating_add(1);
                                continue;
                            }
                        };
                        return Ok(AuthHttpResponse {
                            status: resp.status,
                            headers: resp.headers,
                            body,
                        });
                    }
                }

                let mut base_request = BuiltRequest {
                    meta,
                    url,
                    headers,
                    body,
                    timeout: policy.timeout,
                    cache: crate::cache::CacheSetting::Off,
                    cache_mode: CacheRequestMode::Default,
                    retry: RetrySetting::Inherit,
                    rate_limit: RateLimitPlan::new(),
                    cache_revalidation: None,
                    extensions: Default::default(),
                };

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
                                format!("internal auth recursion detected for requirement `{requirement}`"),
                            ));
                        }

                        AUTH_INTERNAL_STACK.with(|stack| stack.borrow_mut().push(requirement_key));
                        let auth_state_snapshot = self.client.auth_state();
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
                    let mut built = base_request.clone();
                    built.meta.attempt = attempt;

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
                        let transport_req =
                            crate::transport::materialize_transport_request(&built, &auth_materials)
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
                                attempt = attempt.saturating_add(1);
                                continue;
                            }
                        };
                        let body = match read_body_all(resp.body.as_mut(), resp.content_length).await {
                            Ok(body) => body,
                            Err(source) => {
                                if attempt >= policy.max_transport_retries {
                                    return Err(AuthError::new(
                                        AuthErrorKind::AcquireFailed,
                                        source.to_string(),
                                    ));
                                }
                                attempt = attempt.saturating_add(1);
                                continue;
                            }
                        };
                        return Ok(AuthHttpResponse {
                            status: resp.status,
                            headers: resp.headers,
                            body,
                        });
                    }

                    let transport_req = crate::transport::materialize_transport_request(&built, &auth_materials)
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
                            attempt = attempt.saturating_add(1);
                            continue;
                        }
                    };

                    let body = match read_body_all(resp.body.as_mut(), resp.content_length).await {
                        Ok(body) => body,
                        Err(source) => {
                            if attempt >= policy.max_transport_retries {
                                return Err(AuthError::new(
                                    AuthErrorKind::AcquireFailed,
                                    source.to_string(),
                                ));
                            }
                            attempt = attempt.saturating_add(1);
                            continue;
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

async fn read_body_all(
    body: &mut dyn TransportBody,
    content_length: Option<u64>,
) -> Result<Bytes, TransportError> {
    // Sanity cap: au-delà, on évite toute pré-allocation “grosse” basée sur Content-Length.
    const SANITY_CAP: usize = 2 * 1024 * 1024; // 2MB
    const SMALL_START: usize = 8 * 1024;
    const LARGE_START: usize = 64 * 1024;

    let cap = match content_length {
        None => SMALL_START,
        Some(n) => {
            let n_usize = usize::try_from(n).unwrap_or(usize::MAX);
            if n_usize <= SANITY_CAP {
                n_usize.max(SMALL_START)
            } else {
                LARGE_START
            }
        }
    };
    let mut buf = bytes::BytesMut::with_capacity(cap);
    while let Some(chunk) = body.next_chunk().await? {
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::codec::{Format, FormatType, NoContent, format_debug_body, text::Text};

    struct BinaryEncoding;
    impl FormatType for BinaryEncoding {
        const FORMAT_TYPE: Format = Format::Binary;
    }

    #[test]
    fn debug_preview_uses_request_encoder_and_response_decoder_formats() {
        // Request: binary => base64
        let req = Bytes::from_static(&[0x00, 0x01, 0x02]);
        let req_s = format_debug_body::<BinaryEncoding>(&req, 1024);
        assert_eq!(req_s, "AAEC");

        // Response: text => UTF-8
        let resp = Bytes::from_static(b"hello");
        let resp_s = format_debug_body::<Text>(&resp, 1024);
        assert_eq!(resp_s, "hello");

        // sanity: NoContentEncoding is text-format (empty)
        let empty = Bytes::new();
        let s = crate::codec::format_debug_body::<NoContent>(&empty, 1024);
        assert_eq!(s, "");
    }
}
