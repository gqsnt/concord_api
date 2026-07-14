// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub(super) struct ObservedExecutionResponse {
    pub(super) response: ExecutionResponse,
    pub(super) rate_limit_action: RateLimitResponseAction,
}

impl<Cx: ClientContext> ApiClient<Cx> {
    pub(super) async fn run_post_response_hook(&self, ctx: ResponseObservationCtx<'_>) {
        let hook_meta = HookMeta {
            endpoint: ctx.endpoint,
            method: ctx.method,
            url: ctx.url,
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
        mut built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
        stream_request_limit: Option<usize>,
    ) -> Result<ExecutionResponse, ApiClientError> {
        let request_context = built.context();
        let rate_limit_meta = RateLimitContext {
            endpoint: request_context.meta.endpoint,
            method: &request_context.meta.method,
            url: send_ctx.url_str,
            url_host: built.message.url().host_str(),
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
        if let Some(limit) = stream_request_limit {
            let hint = built
                .message
                .body()
                .map(http_body::Body::size_hint)
                .unwrap_or_else(|| {
                    let mut hint = http_body::SizeHint::new();
                    hint.set_exact(0);
                    hint
                });
            // An advisory upper bound is not a delivered length. Only an exact
            // length, or a lower bound already beyond the limit, can reject
            // before streaming; the body limiter owns every other case.
            let known_oversize = hint
                .exact()
                .or_else(|| (hint.lower() > limit as u64).then_some(hint.lower()));
            if let Some(actual) = known_oversize
                && actual > limit as u64
            {
                let terminal_error = ApiClientError::RequestBodyLimitExceeded {
                    ctx: send_ctx.error_ctx.clone(),
                    limit,
                    actual,
                };
                let request_context = built.context();
                let method = request_context.meta.method.clone();
                self.runtime_state
                    .hooks()
                    .request_error(RequestErrorHookContext {
                        meta: HookMeta {
                            endpoint: request_context.meta.endpoint,
                            method: &method,
                            url: send_ctx.url_str,
                            page_index: request_context.meta.page_index,
                            idempotent: request_context.meta.idempotent,
                        },
                        category: terminal_error.category(),
                    })
                    .await;
                return Err(terminal_error);
            }
            if let Some(body) = built.message.body_mut().take() {
                if body.as_bytes().is_some() {
                    *built.message.body_mut() = Some(body);
                } else {
                    *built.message.body_mut() = Some(reqwest::Body::wrap(
                        crate::body::LimitedBody::new(body, limit as u64),
                    ));
                }
            }
        }
        self.debug_planned_request(send_ctx.dbg, &built, send_ctx.url_str);
        let request_context = built.context();
        let pre_send_meta = HookMeta {
            endpoint: request_context.meta.endpoint,
            method: &request_context.meta.method,
            url: send_ctx.url_str,
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
    ) -> Result<ExecutionResponse, ApiClientError> {
        let request_context = built.context();
        let endpoint = request_context.meta.endpoint;
        let method = request_context.meta.method.clone();
        let page_index = request_context.meta.page_index;
        let idempotent = request_context.meta.idempotent;
        let response_context = crate::transport::ResponseContext {
            meta: request_context.meta.clone(),
            logical_url: request_context.logical_url.clone(),
            rate_limit: built.rate_limit.clone(),
        };
        let BuiltRequest {
            message,
            context,
            auth_plan,
            rate_limit: _,
        } = built;
        let native_request =
            crate::transport::materialize_authentication(message, &auth_plan, auth_materials)
                .map_err(|source| ApiClientError::Auth {
                    ctx: ctx.clone(),
                    source,
                })?;

        // One visible execution: exactly one call to reqwest::Client::execute.
        // Reqwest owns any hidden protocol/status resend internally; it does not
        // rerun Concord hooks, rate limiting, or authentication preparation.
        let transport_result = self
            .managed_client
            .execute(native_request, Some(&context))
            .await;
        match transport_result {
            Ok(message) => {
                let error_mapper = self.managed_client.response_error_mapper();
                Ok(ExecutionResponse::new(
                    message,
                    response_context,
                    error_mapper,
                    None,
                ))
            }
            Err(e) => {
                let e = context
                    .body_errors
                    .get()
                    .map(crate::transport::ReqwestError::from)
                    .unwrap_or(e);
                let terminal_error = if let Some(body_error) = e.body_error()
                    && body_error.kind() == crate::body::BodyErrorKind::LimitExceeded
                {
                    ApiClientError::RequestBodyLimitExceeded {
                        ctx: ctx.clone(),
                        limit: body_error.limit().unwrap_or_default() as usize,
                        actual: body_error.observed().unwrap_or_default(),
                    }
                } else {
                    ApiClientError::request_execution(ctx.clone(), e)
                };
                let hook_meta = HookMeta {
                    endpoint,
                    method: &method,
                    url: safe_url,
                    page_index,
                    idempotent,
                };
                self.runtime_state
                    .hooks()
                    .request_error(RequestErrorHookContext {
                        meta: hook_meta,
                        category: terminal_error.category(),
                    })
                    .await;
                Err(terminal_error)
            }
        }
    }

    pub(super) async fn send_and_observe_once(
        &self,
        built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
    ) -> Result<ObservedExecutionResponse, ApiClientError> {
        let resp = self
            .acquire_rate_limit_and_send(
                built,
                send_ctx,
                self.runtime_state.max_request_body_bytes(),
            )
            .await?;
        self.observe_transport_response(resp, send_ctx.url_str, send_ctx.error_ctx)
            .await
    }

    pub(super) async fn observe_transport_response(
        &self,
        response: ExecutionResponse,
        url_str: &str,
        ctx: &ErrorContext,
    ) -> Result<ObservedExecutionResponse, ApiClientError> {
        let observe_ctx = Self::response_observation_ctx(&response, url_str);
        self.run_post_response_hook(observe_ctx).await;
        let rate_limit_action = self.observe_rate_limit_response(observe_ctx, ctx).await?;
        Ok(ObservedExecutionResponse {
            response,
            rate_limit_action,
        })
    }

    pub(super) fn classify_observed_transport_response(
        &self,
        observed: ObservedExecutionResponse,
        dbg: DebugLevel,
        dbg_verbose: bool,
        url_str: &str,
        ctx: &ErrorContext,
        emit_success_debug: bool,
    ) -> Result<ExecutionResponse, ApiClientError> {
        let ObservedExecutionResponse {
            response,
            rate_limit_action,
        } = observed;
        match classify_status(response.status()) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, response.status(), url_str, false);
                    self.debug_sink.response_headers(
                        dbg,
                        crate::debug::SanitizedHeaders::new(response.headers()),
                    );
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: response.status(),
                    headers: Box::new(crate::redaction::sanitize_header_map(response.headers())),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                if emit_success_debug && dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, response.status(), url_str, true);
                    self.debug_sink.response_headers(
                        dbg,
                        crate::debug::SanitizedHeaders::new(response.headers()),
                    );
                }
                Ok(response)
            }
        }
    }

    pub(super) fn limit_response_body(
        mut resp: ExecutionResponse,
        limit: Option<usize>,
        ctx: &ErrorContext,
    ) -> Result<ExecutionResponse, ApiClientError> {
        let Some(limit) = limit else {
            return Ok(resp);
        };
        let limit = u64::try_from(limit).unwrap_or(u64::MAX);
        if let Some(actual) = resp.body.content_length()
            && actual > limit
        {
            return Err(ApiClientError::ResponseTooLarge {
                ctx: ctx.clone(),
                limit: limit as usize,
                actual,
            });
        }
        resp.set_body_limit(limit);
        Ok(resp)
    }

    pub(super) async fn buffer_response(
        resp: ExecutionResponse,
        skip_body: bool,
        ctx: &ErrorContext,
    ) -> Result<BuiltResponse, ApiClientError> {
        let ExecutionResponse { mut body, context } = resp;
        let bytes = if skip_body {
            bytes::Bytes::new()
        } else {
            body.collect_bytes().await.map_err(|source| {
                if source.kind() == crate::body::BodyErrorKind::LimitExceeded {
                    ApiClientError::ResponseBodyLimitExceeded {
                        ctx: ctx.clone(),
                        limit: source.limit().unwrap_or_default() as usize,
                    }
                } else {
                    ApiClientError::response_body_error(ctx.clone(), source)
                }
            })?
        };

        let (status, version, headers, extensions) = body.into_head();
        let mut buffered = http::Response::new(bytes);
        *buffered.status_mut() = status;
        *buffered.version_mut() = version;
        *buffered.headers_mut() = headers;
        *buffered.extensions_mut() = extensions;
        Ok(BuiltResponse::new(buffered, context))
    }

    pub(super) fn response_observation_ctx<'a>(
        resp: &'a ExecutionResponse,
        url_str: &'a str,
    ) -> ResponseObservationCtx<'a> {
        ResponseObservationCtx {
            endpoint: resp.context.meta.endpoint,
            method: &resp.context.meta.method,
            url: url_str,
            url_host: resp.logical_url().host_str(),
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
