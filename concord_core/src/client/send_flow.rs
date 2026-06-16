impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    async fn run_post_response_hook(
        &self,
        endpoint: &'static str,
        method: &http::Method,
        url: &str,
        attempt: u32,
        page_index: u32,
        idempotent: bool,
        status: http::StatusCode,
        headers: &http::HeaderMap,
    ) {
        let hook_meta = HookMeta {
            endpoint,
            method,
            url,
            attempt,
            page_index,
            idempotent,
        };
        self.runtime_state
            .hooks()
            .post_response(PostResponseHookContext {
                meta: hook_meta,
                status,
                headers,
            })
            .await;
    }

    async fn acquire_rate_limit_and_send(
        &self,
        built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
    ) -> Result<TransportResponse, ApiClientError> {
        let rate_limit_meta = RateLimitContext {
            endpoint: built.meta.endpoint,
            method: &built.meta.method,
            url: built.url.as_str(),
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
        self.send_built_request(built, send_ctx.error_ctx).await
    }

    async fn observe_rate_limit_response(
        &self,
        endpoint: &'static str,
        method: http::Method,
        url: String,
        url_host: Option<String>,
        attempt: u32,
        page_index: u32,
        idempotent: bool,
        plan: crate::rate_limit::RateLimitPlan,
        status: http::StatusCode,
        headers: http::HeaderMap,
    ) -> Result<RateLimitResponseAction, ApiClientError> {
        let rate_limit_meta = RateLimitContext {
            endpoint,
            method: &method,
            url: &url,
            url_host: url_host.as_deref(),
            attempt,
            page_index,
            idempotent,
            plan: &plan,
        };
        self.runtime_state
            .rate_limiter()
            .on_response(RateLimitResponseContext {
                meta: rate_limit_meta,
                status,
                headers: &headers,
        })
        .await
    }

    async fn send_built_request(
        &self,
        built: BuiltRequest,
        ctx: &ErrorContext,
    ) -> Result<TransportResponse, ApiClientError> {
        let endpoint = built.meta.endpoint;
        let method = built.meta.method.clone();
        let attempt = built.meta.attempt;
        let page_index = built.meta.page_index;
        let idempotent = built.meta.idempotent;
        let url = built.url.as_str().to_owned();

        match self.transport.send(built).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
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
        self.run_post_response_hook(
            resp.meta.endpoint,
            &resp.meta.method,
            resp.url.as_str(),
            resp.meta.attempt,
            resp.meta.page_index,
            resp.meta.idempotent,
            resp.status,
            &resp.headers,
        )
        .await;
        let rate_limit_action = self
            .observe_rate_limit_response(
                resp.meta.endpoint,
                resp.meta.method.clone(),
                resp.url.as_str().to_owned(),
                resp.url.host_str().map(ToOwned::to_owned),
                resp.meta.attempt,
                resp.meta.page_index,
                resp.meta.idempotent,
                resp.rate_limit.clone(),
                resp.status,
                resp.headers.clone(),
            )
            .await?;
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
                let bytes = read_body_all(resp.body.as_mut(), resp.content_length)
                    .await
                    .map_err(|e| ApiClientError::Transport {
                        ctx: ctx.clone(),
                        source: e,
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

    async fn send_and_classify_with_inflight(
        &self,
        built: BuiltRequest,
        inflight_key: Option<RequestKey>,
        send_ctx: SendClassifyCtx<'_>,
    ) -> Result<BuiltResponse, ApiClientError> {
        if let Some(key) = inflight_key {
            let join = self
                .runtime_state
                .inflight_registry()
                .join_or_lead(key)
                .await;
            if join.is_leader() {
                let result = self.send_and_classify_once(built, send_ctx).await;
                let shared = match &result {
                    Ok(resp) => SharedSendResult::Ok(resp.clone()),
                    Err(err) => SharedSendResult::Err(SharedSendError::from_api_error(err)),
                };
                join.complete(self.runtime_state.inflight_registry(), shared)
                    .await;
                result
            } else {
                match join.wait().await {
                    SharedSendResult::Ok(resp) => Ok(resp),
                    SharedSendResult::Err(err) => {
                        Err(err.into_api_error(send_ctx.error_ctx.clone()))
                    }
                }
            }
        } else {
            self.send_and_classify_once(built, send_ctx).await
        }
    }

    async fn send_and_classify_once(
        &self,
        built: BuiltRequest,
        send_ctx: SendClassifyCtx<'_>,
    ) -> Result<BuiltResponse, ApiClientError> {
        let has_cache_revalidation = built.cache_revalidation.is_some();
        let transport_resp = self.acquire_rate_limit_and_send(built, send_ctx).await?;
        if transport_resp.status == http::StatusCode::NOT_MODIFIED && has_cache_revalidation {
            self.run_post_response_hook(
                transport_resp.meta.endpoint,
                &transport_resp.meta.method,
                transport_resp.url.as_str(),
                transport_resp.meta.attempt,
                transport_resp.meta.page_index,
                transport_resp.meta.idempotent,
                transport_resp.status,
                &transport_resp.headers,
            )
            .await;
            let _ = self
                .observe_rate_limit_response(
                    transport_resp.meta.endpoint,
                    transport_resp.meta.method.clone(),
                    transport_resp.url.as_str().to_owned(),
                    transport_resp.url.host_str().map(ToOwned::to_owned),
                    transport_resp.meta.attempt,
                    transport_resp.meta.page_index,
                    transport_resp.meta.idempotent,
                    transport_resp.rate_limit.clone(),
                    transport_resp.status,
                    transport_resp.headers.clone(),
                )
                .await?;
            return Ok(BuiltResponse {
                meta: transport_resp.meta.clone(),
                url: transport_resp.url.clone(),
                status: transport_resp.status,
                headers: transport_resp.headers.clone(),
                body: Bytes::new(),
                rate_limit: transport_resp.rate_limit.clone(),
            });
        }
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

}
