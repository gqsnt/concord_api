// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub(super) async fn run_post_response_hook(&self, ctx: ResponseObservationCtx<'_>) {
        let hook_meta = HookMeta {
            endpoint: ctx.endpoint,
            method: ctx.method,
            url: ctx.url,
            attempt: ctx.attempt,
            page_index: ctx.page_index,
            idempotent: ctx.idempotent,
        };
        self.runtime_state
            .hooks()
            .post_response(PostResponseHookContext {
                meta: hook_meta,
                status: ctx.status,
                headers: crate::debug::SanitizedHeaders::new(ctx.headers),
            })
            .await;
    }

    pub(super) async fn acquire_rate_limit_and_send(
        &self,
        built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
        stream_request_limit: Option<usize>,
    ) -> Result<AttemptResponse, ApiClientError> {
        let request_context = built.context();
        let rate_limit_meta = RateLimitContext {
            endpoint: request_context.meta.endpoint,
            method: &request_context.meta.method,
            url: send_ctx.url_str,
            url_host: built.message.uri().host(),
            attempt: request_context.meta.attempt,
            page_index: request_context.meta.page_index,
            idempotent: request_context.meta.idempotent,
            max_cooldown: self.runtime_state.max_rate_limit_cooldown(),
            plan: &built.rate_limit,
        };
        let _permit = self
            .runtime_state
            .rate_limiter()
            .acquire(rate_limit_meta)
            .await
            .map_err(|err| {
                wrap_rate_limit_error(
                    send_ctx.error_ctx.clone(),
                    crate::rate_limit::RateLimitErrorKind::AcquireFailed,
                    "rate-limit acquire failed",
                    err,
                )
            })?;
        if built.stream_like
            && let Some(limit) = stream_request_limit
            && let Some(actual) = http_body::Body::size_hint(built.message.body()).upper()
            && actual > limit as u64
        {
            return Err(ApiClientError::RequestBodyLimitExceeded {
                ctx: send_ctx.error_ctx.clone(),
                limit,
                actual,
            });
        }
        self.debug_planned_request(send_ctx.dbg, &built, send_ctx.url_str);
        let request_context = built.context();
        let pre_send_meta = HookMeta {
            endpoint: request_context.meta.endpoint,
            method: &request_context.meta.method,
            url: send_ctx.url_str,
            attempt: request_context.meta.attempt,
            page_index: request_context.meta.page_index,
            idempotent: request_context.meta.idempotent,
        };
        self.runtime_state
            .hooks()
            .pre_send(PreSendHookContext {
                meta: pre_send_meta,
                headers: crate::debug::SanitizedHeaders::new(built.message.headers()),
            })
            .await?;
        self.send_built_request(
            built,
            send_ctx.url_str,
            send_ctx.auth_materials,
            send_ctx.error_ctx,
            stream_request_limit,
        )
        .await
    }

    pub(super) async fn observe_rate_limit_response(
        &self,
        ctx: ResponseObservationCtx<'_>,
        error_ctx: &ErrorContext,
    ) -> Result<RateLimitResponseAction, ApiClientError> {
        let rate_limit_meta = RateLimitContext {
            endpoint: ctx.endpoint,
            method: ctx.method,
            url: ctx.url,
            url_host: ctx.url_host,
            attempt: ctx.attempt,
            page_index: ctx.page_index,
            idempotent: ctx.idempotent,
            max_cooldown: self.runtime_state.max_rate_limit_cooldown(),
            plan: ctx.plan,
        };
        self.runtime_state
            .rate_limiter()
            .on_response(RateLimitResponseContext {
                meta: rate_limit_meta,
                status: ctx.status,
                headers: crate::debug::SanitizedHeaders::new(ctx.headers),
                max_cooldown: self.runtime_state.max_rate_limit_cooldown(),
            })
            .await
            .map_err(|err| {
                wrap_rate_limit_error(
                    error_ctx.clone(),
                    crate::rate_limit::RateLimitErrorKind::ResponseActionFailed,
                    "rate-limit response action failed",
                    err,
                )
            })
    }

    pub(super) async fn send_built_request(
        &self,
        built: BuiltRequest,
        safe_url: &str,
        auth_materials: &[crate::auth::AuthTransportMaterial],
        ctx: &ErrorContext,
        stream_request_limit: Option<usize>,
    ) -> Result<AttemptResponse, ApiClientError> {
        let request_context = built.context();
        let endpoint = request_context.meta.endpoint;
        let method = request_context.meta.method.clone();
        let attempt = request_context.meta.attempt;
        let page_index = request_context.meta.page_index;
        let idempotent = request_context.meta.idempotent;
        let request_url = url::Url::parse(built.message.uri().to_string().as_str())
            .expect("built request URI was validated during construction");
        let response_context = crate::transport::ResponseContext {
            meta: request_context.meta.clone(),
            request_url,
            rate_limit: built.rate_limit.clone(),
        };
        let transport_req = crate::transport::materialize_transport_request(
            built,
            auth_materials,
            stream_request_limit,
        )
        .map_err(|source| ApiClientError::Auth {
            ctx: ctx.clone(),
            source,
        })?;

        match self.transport.send(transport_req).await {
            Ok(message) => Ok(AttemptResponse {
                message,
                context: response_context,
            }),
            Err(e) => {
                if let Some(body_error) = e.body_error()
                    && body_error.kind() == crate::body::BodyErrorKind::LimitExceeded
                {
                    return Err(ApiClientError::RequestBodyLimitExceeded {
                        ctx: ctx.clone(),
                        limit: body_error.limit().unwrap_or_default() as usize,
                        actual: body_error.observed().unwrap_or_default(),
                    });
                }
                let hook_meta = HookMeta {
                    endpoint,
                    method: &method,
                    url: safe_url,
                    attempt,
                    page_index,
                    idempotent,
                };
                self.runtime_state
                    .hooks()
                    .transport_error(TransportErrorHookContext {
                        meta: hook_meta,
                        error: &e,
                    })
                    .await;
                Err(ApiClientError::Transport {
                    ctx: ctx.clone(),
                    source: e,
                })
            }
        }
    }

    // Lifecycle classification carries request metadata separately to preserve
    // the fixed attempt ordering; grouping it would be a behavioral refactor.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn classify_transport_response(
        &self,
        resp: AttemptResponse,
        skip_body: bool,
        dbg: DebugLevel,
        dbg_verbose: bool,
        _dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
    ) -> Result<BuiltResponse, ApiClientError> {
        let observe_ctx = Self::response_observation_ctx(&resp, url_str);
        self.run_post_response_hook(observe_ctx).await;
        let rate_limit_action = self.observe_rate_limit_response(observe_ctx, ctx).await?;
        match classify_status(resp.status()) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status(), url_str, false);
                    self.debug_sink
                        .response_headers(dbg, crate::debug::SanitizedHeaders::new(resp.headers()));
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status(),
                    headers: Box::new(crate::redaction::sanitize_header_map(resp.headers())),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                let (parts, mut body) = resp.message.into_parts();
                let content_length = content_length(&parts.headers);
                let bytes = if skip_body {
                    Bytes::new()
                } else {
                    read_body_all_limited(
                        &mut body,
                        content_length,
                        self.runtime_state.max_response_body_bytes(),
                    )
                    .await
                    .map_err(|e| match e {
                        BodyReadError::Body(source) => {
                            ApiClientError::response_body_error(ctx.clone(), source)
                        }
                        BodyReadError::ContentLengthTooLarge { limit, actual } => {
                            ApiClientError::ResponseTooLarge {
                                ctx: ctx.clone(),
                                limit,
                                actual,
                            }
                        }
                        BodyReadError::LimitExceeded { limit } => {
                            ApiClientError::ResponseBodyLimitExceeded {
                                ctx: ctx.clone(),
                                limit,
                            }
                        }
                    })?
                };
                Ok(BuiltResponse::new(
                    http::Response::from_parts(parts, bytes),
                    resp.context,
                ))
            }
        }
    }

    pub(super) async fn send_and_classify_once(
        &self,
        built: BuiltRequest,
        skip_body: bool,
        send_ctx: SendClassifyCtx<'_>,
    ) -> Result<BuiltResponse, ApiClientError> {
        let transport_resp = self
            .acquire_rate_limit_and_send(
                built,
                send_ctx,
                self.runtime_state.max_stream_request_body_bytes(),
            )
            .await?;
        self.classify_transport_response(
            transport_resp,
            skip_body,
            send_ctx.dbg,
            send_ctx.dbg_verbose,
            send_ctx.dbg_vv,
            send_ctx.url_str,
            send_ctx.error_ctx,
        )
        .await
    }

    pub(super) async fn send_and_classify_transport_once(
        &self,
        built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
        response_limit: Option<usize>,
    ) -> Result<AttemptResponse, ApiClientError> {
        let transport_resp = self
            .acquire_rate_limit_and_send(
                built,
                send_ctx,
                self.runtime_state.max_stream_request_body_bytes(),
            )
            .await?;
        self.observe_and_classify_transport_response(
            transport_resp,
            send_ctx.dbg,
            send_ctx.dbg_verbose,
            send_ctx.dbg_vv,
            send_ctx.url_str,
            send_ctx.error_ctx,
            response_limit,
        )
        .await
    }

    // Streaming classification mirrors the buffered path so hook, rate-limit,
    // and debug ordering stays explicit at the call site.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn observe_and_classify_transport_response(
        &self,
        resp: AttemptResponse,
        dbg: DebugLevel,
        dbg_verbose: bool,
        _dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
        response_limit: Option<usize>,
    ) -> Result<AttemptResponse, ApiClientError> {
        let observe_ctx = Self::response_observation_ctx(&resp, url_str);
        self.run_post_response_hook(observe_ctx).await;
        let rate_limit_action = self.observe_rate_limit_response(observe_ctx, ctx).await?;
        match classify_status(resp.status()) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status(), url_str, false);
                    self.debug_sink
                        .response_headers(dbg, crate::debug::SanitizedHeaders::new(resp.headers()));
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status(),
                    headers: Box::new(crate::redaction::sanitize_header_map(resp.headers())),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status(), url_str, true);
                    self.debug_sink
                        .response_headers(dbg, crate::debug::SanitizedHeaders::new(resp.headers()));
                }
                if let (Some(limit), Some(actual)) =
                    (response_limit, content_length(resp.headers()))
                    && actual > limit as u64
                {
                    return Err(ApiClientError::ResponseTooLarge {
                        ctx: ctx.clone(),
                        limit,
                        actual,
                    });
                }
                Ok(resp)
            }
        }
    }

    pub(super) fn response_observation_ctx<'a>(
        resp: &'a AttemptResponse,
        url_str: &'a str,
    ) -> ResponseObservationCtx<'a> {
        ResponseObservationCtx {
            endpoint: resp.context.meta.endpoint,
            method: &resp.context.meta.method,
            url: url_str,
            url_host: resp.context.request_url.host_str(),
            attempt: resp.context.meta.attempt,
            page_index: resp.context.meta.page_index,
            idempotent: resp.context.meta.idempotent,
            plan: &resp.context.rate_limit,
            status: resp.status(),
            headers: resp.headers(),
        }
    }

    pub(super) fn header_matches_media_type(
        value: Option<&http::HeaderValue>,
        expected: &'static str,
    ) -> bool {
        value
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(|base| base.trim().eq_ignore_ascii_case(expected))
            .unwrap_or(false)
    }
}

fn content_length(headers: &http::HeaderMap) -> Option<u64> {
    headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn wrap_rate_limit_error(
    ctx: crate::error::ErrorContext,
    kind: crate::rate_limit::RateLimitErrorKind,
    message: &'static str,
    err: crate::error::ApiClientError,
) -> crate::error::ApiClientError {
    match err {
        crate::error::ApiClientError::RateLimit { .. } => err,
        other => crate::error::ApiClientError::rate_limit_with_source(ctx, kind, message, other),
    }
}
