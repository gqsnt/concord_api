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
        built: crate::transport::AuthCollisionValidatedBuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
        stream_request_limit: Option<usize>,
    ) -> Result<TransportResponse, ApiClientError> {
        let rate_limit_meta = RateLimitContext {
            endpoint: built.meta.endpoint,
            method: &built.meta.method,
            url: send_ctx.url_str,
            url_host: built.url.host_str(),
            attempt: built.meta.attempt,
            page_index: built.meta.page_index,
            idempotent: built.meta.idempotent,
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
        if built.body.is_stream()
            && let Some(limit) = stream_request_limit
            && let Some(actual) = built.body.size_hint().upper()
            && actual > limit as u64
        {
            return Err(ApiClientError::RequestBodyLimitExceeded {
                ctx: send_ctx.error_ctx.clone(),
                limit,
                actual,
            });
        }
        let pre_send_meta = HookMeta {
            endpoint: built.meta.endpoint,
            method: &built.meta.method,
            url: send_ctx.url_str,
            attempt: built.meta.attempt,
            page_index: built.meta.page_index,
            idempotent: built.meta.idempotent,
        };
        self.runtime_state
            .hooks()
            .pre_send(PreSendHookContext {
                meta: pre_send_meta,
                headers: crate::debug::SanitizedHeaders::new(&built.headers),
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
        built: crate::transport::AuthCollisionValidatedBuiltRequest,
        safe_url: &str,
        auth_materials: &[crate::auth::AuthTransportMaterial],
        ctx: &ErrorContext,
        stream_request_limit: Option<usize>,
    ) -> Result<TransportResponse, ApiClientError> {
        let endpoint = built.meta.endpoint;
        let method = built.meta.method.clone();
        let attempt = built.meta.attempt;
        let page_index = built.meta.page_index;
        let idempotent = built.meta.idempotent;
        let request_url = built.url.clone();
        let transport_req = crate::transport::materialize_transport_request_validated(
            built,
            auth_materials,
            stream_request_limit,
        )
        .map_err(|source| ApiClientError::Auth {
            ctx: ctx.clone(),
            source,
        })?;

        match self.transport.send(transport_req).await {
            Ok(mut resp) => {
                resp.url = request_url;
                Ok(resp)
            }
            Err(e) => {
                if let Some(_codec_error) =
                    e.source_error().downcast_ref::<crate::codec::CodecError>()
                {
                    return Err(ApiClientError::Codec {
                        ctx: ctx.clone(),
                        source: Box::new(crate::codec::CodecError::new(
                            "request body encoding failed",
                        )),
                    });
                }
                if let Some(limit_error) = e
                    .source_error()
                    .downcast_ref::<crate::transport::StreamBodyLimitError>()
                    && matches!(
                        limit_error.direction,
                        crate::transport::StreamLimitDirection::Request
                    )
                {
                    return Err(ApiClientError::RequestBodyLimitExceeded {
                        ctx: ctx.clone(),
                        limit: limit_error.limit,
                        actual: limit_error.seen as u64,
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
        mut resp: TransportResponse,
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
        match classify_status(resp.status) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, false);
                    self.debug_sink
                        .response_headers(dbg, crate::debug::SanitizedHeaders::new(&resp.headers));
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status,
                    headers: Box::new(crate::redaction::sanitize_header_map(&resp.headers)),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                let bytes = if skip_body {
                    Bytes::new()
                } else {
                    read_body_all_limited(
                        resp.body.as_mut(),
                        resp.content_length,
                        self.runtime_state.max_response_body_bytes(),
                    )
                    .await
                    .map_err(|e| match e {
                        BodyReadError::Transport(source) => {
                            ApiClientError::response_body_read_transport_error(ctx.clone(), source)
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
                Ok(BuiltResponse {
                    meta: resp.meta,
                    url: resp.url,
                    status: resp.status,
                    headers: resp.headers,
                    body: bytes,
                    rate_limit: resp.rate_limit,
                })
            }
        }
    }

    pub(super) async fn send_and_classify_once(
        &self,
        built: crate::transport::AuthCollisionValidatedBuiltRequest,
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
        built: crate::transport::AuthCollisionValidatedBuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
        response_limit: Option<usize>,
    ) -> Result<TransportResponse, ApiClientError> {
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
        resp: TransportResponse,
        dbg: DebugLevel,
        dbg_verbose: bool,
        _dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
        response_limit: Option<usize>,
    ) -> Result<TransportResponse, ApiClientError> {
        let observe_ctx = Self::response_observation_ctx(&resp, url_str);
        self.run_post_response_hook(observe_ctx).await;
        let rate_limit_action = self.observe_rate_limit_response(observe_ctx, ctx).await?;
        match classify_status(resp.status) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, false);
                    self.debug_sink
                        .response_headers(dbg, crate::debug::SanitizedHeaders::new(&resp.headers));
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status,
                    headers: Box::new(crate::redaction::sanitize_header_map(&resp.headers)),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, true);
                    self.debug_sink
                        .response_headers(dbg, crate::debug::SanitizedHeaders::new(&resp.headers));
                }
                if let (Some(limit), Some(actual)) = (response_limit, resp.content_length)
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
        resp: &'a TransportResponse,
        url_str: &'a str,
    ) -> ResponseObservationCtx<'a> {
        ResponseObservationCtx {
            endpoint: resp.meta.endpoint,
            method: &resp.meta.method,
            url: url_str,
            url_host: resp.url.host_str(),
            attempt: resp.meta.attempt,
            page_index: resp.meta.page_index,
            idempotent: resp.meta.idempotent,
            plan: &resp.rate_limit,
            status: resp.status,
            headers: &resp.headers,
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
