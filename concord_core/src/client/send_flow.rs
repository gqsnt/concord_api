impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    async fn run_post_response_hook(
        &self,
        ctx: ResponseObservationCtx<'_>,
    ) {
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
                headers: ctx.headers,
            })
            .await;
    }

    async fn acquire_rate_limit_and_send(
        &self,
        built: BuiltRequest,
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
            plan: &built.rate_limit,
        };
        let _permit = self
            .runtime_state
            .rate_limiter()
            .acquire(rate_limit_meta)
            .await?;
        if let (Some(limit), Some(hint)) = (stream_request_limit, built.stream_size_hint) {
            if let Some(actual) = hint.upper() {
                if actual > limit as u64 {
                    return Err(ApiClientError::RequestBodyLimitExceeded {
                        ctx: send_ctx.error_ctx.clone(),
                        limit,
                        actual,
                    });
                }
            }
        }
        let pre_send_meta = HookMeta {
            endpoint: built.meta.endpoint,
            method: &built.meta.method,
            url: built.url.as_str(),
            attempt: built.meta.attempt,
            page_index: built.meta.page_index,
            idempotent: built.meta.idempotent,
        };
        self.runtime_state
            .hooks()
            .pre_send(PreSendHookContext {
                meta: pre_send_meta,
                headers: &built.headers,
            })
            .await?;
        self.send_built_request(
            built,
            send_ctx.auth_materials,
            send_ctx.error_ctx,
            stream_request_limit,
        )
        .await
    }

    async fn observe_rate_limit_response(
        &self,
        ctx: ResponseObservationCtx<'_>,
    ) -> Result<RateLimitResponseAction, ApiClientError> {
        let rate_limit_meta = RateLimitContext {
            endpoint: ctx.endpoint,
            method: ctx.method,
            url: ctx.url,
            url_host: ctx.url_host,
            attempt: ctx.attempt,
            page_index: ctx.page_index,
            idempotent: ctx.idempotent,
            plan: ctx.plan,
        };
        self.runtime_state
            .rate_limiter()
            .on_response(RateLimitResponseContext {
                meta: rate_limit_meta,
                status: ctx.status,
                headers: ctx.headers,
            })
            .await
    }

    async fn send_built_request(
        &self,
        built: BuiltRequest,
        auth_materials: &[crate::auth::AuthTransportMaterial],
        ctx: &ErrorContext,
        stream_request_limit: Option<usize>,
    ) -> Result<TransportResponse, ApiClientError> {
        let endpoint = built.meta.endpoint;
        let method = built.meta.method.clone();
        let attempt = built.meta.attempt;
        let page_index = built.meta.page_index;
        let idempotent = built.meta.idempotent;
        let url = built.debug_url();
        let request_url = built.url.clone();
        let transport_req =
            crate::transport::materialize_transport_request(
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
                if let Some(_codec_error) = e
                    .source_error()
                    .downcast_ref::<crate::codec::CodecError>()
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
                {
                    if matches!(
                        limit_error.direction,
                        crate::transport::StreamLimitDirection::Request
                    ) {
                        return Err(ApiClientError::RequestBodyLimitExceeded {
                            ctx: ctx.clone(),
                            limit: limit_error.limit,
                            actual: limit_error.seen as u64,
                        });
                    }
                }
                let hook_meta = HookMeta {
                    endpoint,
                    method: &method,
                    url: &url,
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

    async fn classify_transport_response(
        &self,
        mut resp: TransportResponse,
        dbg: DebugLevel,
        dbg_verbose: bool,
        _dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
    ) -> Result<BuiltResponse, ApiClientError> {
        let observe_ctx = Self::response_observation_ctx(&resp, url_str);
        self.run_post_response_hook(observe_ctx).await;
        let rate_limit_action = self.observe_rate_limit_response(observe_ctx).await?;
        match classify_status(resp.status) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, false);
                    self.debug_sink.response_headers(dbg, &resp.headers);
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status,
                    headers: Box::new(resp.headers),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                let bytes = read_body_all_limited(
                    resp.body.as_mut(),
                    resp.content_length,
                    self.runtime_state.max_response_body_bytes(),
                )
                .await
                .map_err(|e| match e {
                    BodyReadError::Transport(source) => ApiClientError::Transport {
                        ctx: ctx.clone(),
                        source,
                    },
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
                })?;
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

    async fn send_and_classify_stream_once<M>(
        &self,
        built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
        response_limit: Option<usize>,
    ) -> Result<crate::stream_response::StreamResponse<M>, ApiClientError>
    where
        M: crate::media::MediaType,
    {
        let transport_resp = self
            .acquire_rate_limit_and_send(
                built,
                send_ctx,
                self.runtime_state.max_stream_request_body_bytes(),
            )
            .await?;
        self.classify_transport_stream_response::<M>(
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

    async fn classify_transport_stream_response<M>(
        &self,
        resp: TransportResponse,
        dbg: DebugLevel,
        dbg_verbose: bool,
        _dbg_vv: bool,
        url_str: &str,
        ctx: &ErrorContext,
        response_limit: Option<usize>,
    ) -> Result<crate::stream_response::StreamResponse<M>, ApiClientError>
    where
        M: crate::media::MediaType,
    {
        let observe_ctx = Self::response_observation_ctx(&resp, url_str);
        self.run_post_response_hook(observe_ctx).await;
        let rate_limit_action = self.observe_rate_limit_response(observe_ctx).await?;
        match classify_status(resp.status) {
            ResponseClass::HttpStatusError => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, false);
                    self.debug_sink.response_headers(dbg, &resp.headers);
                }
                Err(ApiClientError::HttpStatus {
                    ctx: ctx.clone(),
                    status: resp.status,
                    headers: Box::new(resp.headers),
                    rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                        .then_some(Box::new(rate_limit_action)),
                })
            }
            ResponseClass::Success => {
                if dbg_verbose {
                    self.debug_sink
                        .response_status(dbg, resp.status, url_str, true);
                    self.debug_sink.response_headers(dbg, &resp.headers);
                }
                if !Self::header_matches_media_type(resp.headers.get(CONTENT_TYPE), M::CONTENT_TYPE)
                {
                    return Err(ApiClientError::PolicyViolation {
                        ctx: ctx.clone(),
                        msg: "stream response content type did not match expected media type",
                    });
                }
                if let (Some(limit), Some(actual)) = (response_limit, resp.content_length) {
                    if actual > limit as u64 {
                        return Err(ApiClientError::ResponseTooLarge {
                            ctx: ctx.clone(),
                            limit,
                            actual,
                        });
                    }
                }
                Ok(crate::stream_response::StreamResponse::new(resp, response_limit))
            }
        }
    }

    async fn send_and_classify_once(
        &self,
        built: BuiltRequest,
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
            send_ctx.dbg,
            send_ctx.dbg_verbose,
            send_ctx.dbg_vv,
            send_ctx.url_str,
            send_ctx.error_ctx,
        )
        .await
    }

    fn response_observation_ctx<'a>(
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

    fn header_matches_media_type(
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
