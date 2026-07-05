#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthRejectionOutcome {
    NotProtected,
    Retry,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttemptFamily {
    Buffered { skip_body: bool },
    Stream { response_limit: Option<usize> },
    Sse { response_limit: Option<usize> },
    Multipart { response_limit: Option<usize> },
}

enum AttemptTransportSuccess {
    Buffered(BuiltResponse),
    Transport(TransportResponse),
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_attempt_request(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        args: &mut crate::endpoint::RequestArgs,
        meta: RequestMeta,
    ) -> Result<BuiltRequest, ApiClientError> {
        self.build_request_from_plan(plan, args, meta)
    }

    async fn prepare_auth(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        auth_state: &Cx::AuthState,
        executor: &dyn AuthHttpExecutor,
        built: &mut BuiltRequest,
    ) -> Result<AuthPreparation, ApiClientError> {
        self.prepare_auth_plan(plan, auth_state, executor, built).await
    }

    async fn handle_auth_rejection(
        &self,
        ctx: AuthRejectionCtx<'_, Cx, T>,
    ) -> Result<AuthRejectionOutcome, ApiClientError> {
        if !Self::is_protected_auth_rejection(ctx.plan, ctx.status) {
            return Ok(AuthRejectionOutcome::NotProtected);
        }
        if self.auth_retry_requested(ctx).await? {
            return Ok(AuthRejectionOutcome::Retry);
        }
        Ok(AuthRejectionOutcome::Terminal)
    }

    fn is_protected_auth_rejection(
        plan: &crate::endpoint::RequestPlanView,
        status: StatusCode,
    ) -> bool {
        matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            && !plan.endpoint.policy.auth.requirements.is_empty()
    }

    fn decide_retry(
        &self,
        err: &ApiClientError,
        retry_config: &RetrySetting,
        retry_ctx: &RetryContext<'_>,
        retry_count: u32,
    ) -> Result<Option<Duration>, ApiClientError> {
        let Some(mut delay) = self.retry_delay(retry_config, retry_ctx, retry_count)? else {
            return Ok(None);
        };
        if Self::rate_limit_action_from_error(err)
            .is_some_and(|action| action.delay_handled_by_rate_limiter())
        {
            delay = Duration::ZERO;
        }
        Ok(Some(delay))
    }

    async fn drive_attempts(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        args: &mut crate::endpoint::RequestArgs,
        ctx: ErrorContext,
        dbg: DebugLevel,
        family: AttemptFamily,
    ) -> Result<AttemptTransportSuccess, ApiClientError> {
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();
        let is_replayable = plan.replayability.is_replayable();
        if let RetrySetting::Config(config) = &plan.endpoint.policy.retry {
            config.validate(ctx.clone())?;
        }
        let base_attempt: u32 = plan.overrides.attempt;
        let max_auth_retries = self.runtime_state.max_auth_retries();
        let auth_state_snapshot = self.try_auth_state().map_err(|source| ApiClientError::Auth {
            ctx: ctx.clone(),
            source,
        })?;
        let auth_http = ClientAuthHttpExecutor { client: self };
        let mut attempt_index: u32 = 0;
        let mut transport_retry_index: u32 = 0;
        let mut auth_retry_index: u32 = 0;

        loop {
            let current_attempt = checked_attempt(base_attempt, attempt_index, &ctx)?;
            let meta = plan
                .endpoint
                .meta
                .request_meta(current_attempt, plan.overrides.page_index);
            let mut built = self.build_attempt_request(plan, args, meta)?;
            let auth_attempt = self
                .prepare_auth(plan, &auth_state_snapshot, &auth_http, &mut built)
                .await?;
            crate::transport::validate_transport_auth_collisions(&built).map_err(|source| {
                ApiClientError::Auth {
                    ctx: ctx.clone(),
                    source,
                }
            })?;
            if is_replayable
                && matches!(
                    &built.body,
                    crate::transport::TransportRequestBody::Stream(_)
                )
            {
                return Err(ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "replayable request plan cannot use a non-replayable body plan",
                });
            }
            let url_str = built.debug_url();

            self.debug_planned_request(dbg, plan, &built, &url_str);
            let retry_config = built.retry.clone();
            let retry_request_headers = built.headers.clone();
            let send_result = match family {
                AttemptFamily::Buffered { skip_body } => self
                    .send_and_classify_once(
                        built,
                        skip_body,
                        SendClassifyCtx {
                            dbg,
                            dbg_verbose,
                            dbg_vv,
                            url_str: &url_str,
                            error_ctx: &ctx,
                            auth_materials: &auth_attempt.materials,
                        },
                    )
                    .await
                    .map(AttemptTransportSuccess::Buffered),
                AttemptFamily::Stream { response_limit }
                | AttemptFamily::Sse { response_limit }
                | AttemptFamily::Multipart { response_limit } => self
                    .send_and_classify_transport_once(
                        built,
                        SendClassifyCtx {
                            dbg,
                            dbg_verbose,
                            dbg_vv,
                            url_str: &url_str,
                            error_ctx: &ctx,
                            auth_materials: &auth_attempt.materials,
                        },
                        response_limit,
                    )
                    .await
                    .map(AttemptTransportSuccess::Transport),
            };

            match send_result {
                Ok(resp) => {
                    let (response_status, response_meta, response_headers) = match &resp {
                        AttemptTransportSuccess::Buffered(resp) => {
                            (resp.status, &resp.meta, Some(&resp.headers))
                        }
                        AttemptTransportSuccess::Transport(resp) => {
                            (resp.status, &resp.meta, Some(&resp.headers))
                        }
                    };
                    if !is_replayable && Self::is_protected_auth_rejection(plan, response_status) {
                        return Err(ApiClientError::Auth {
                            ctx: ctx.clone(),
                            source: AuthError::new(
                                AuthErrorKind::ProviderRejected,
                                "auth challenge rejected",
                            ),
                        });
                    }
                    match self
                        .handle_auth_rejection(AuthRejectionCtx {
                            plan,
                            auth_state: &auth_state_snapshot,
                            auth_http: &auth_http,
                            meta: response_meta,
                            status: response_status,
                            headers: response_headers.expect("classified response missing headers"),
                            auth_attempt: &auth_attempt.summary,
                        })
                        .await?
                    {
                        AuthRejectionOutcome::Retry => {
                            if !is_replayable {
                                return Err(ApiClientError::Auth {
                                    ctx: ctx.clone(),
                                    source: AuthError::new(
                                        AuthErrorKind::ProviderRejected,
                                        "auth challenge rejected",
                                    ),
                                });
                            }
                            if auth_retry_index >= max_auth_retries {
                                return Err(ApiClientError::Auth {
                                    ctx: ctx.clone(),
                                    source: AuthError::new(
                                        AuthErrorKind::ProviderRejected,
                                        format!(
                                            "auth retry budget exhausted (max_auth_retries={max_auth_retries})"
                                        ),
                                    ),
                                });
                            }
                            auth_retry_index = next_attempt_counter(auth_retry_index, &ctx)?;
                            attempt_index = next_attempt_counter(attempt_index, &ctx)?;
                            continue;
                        }
                        AuthRejectionOutcome::Terminal => {
                            return Err(ApiClientError::Auth {
                                ctx: ctx.clone(),
                                source: AuthError::new(
                                    AuthErrorKind::ProviderRejected,
                                    "auth challenge rejected",
                                ),
                            });
                        }
                        AuthRejectionOutcome::NotProtected => {}
                    }
                    return Ok(resp);
                }
                Err(err) => {
                    if matches!(
                        &err,
                        ApiClientError::ResponseTooLarge { .. }
                            | ApiClientError::ResponseBodyLimitExceeded { .. }
                    ) {
                        return Err(err);
                    }
                    if let ApiClientError::HttpStatus { status, headers, .. } = &err {
                        if !is_replayable && Self::is_protected_auth_rejection(plan, *status) {
                            return Err(ApiClientError::Auth {
                                ctx: ctx.clone(),
                                source: AuthError::new(
                                    AuthErrorKind::ProviderRejected,
                                    "auth challenge rejected",
                                ),
                            });
                        }
                        let response_meta =
                            plan.endpoint.meta.request_meta(current_attempt, plan.overrides.page_index);
                        match self
                            .handle_auth_rejection(AuthRejectionCtx {
                                plan,
                                auth_state: &auth_state_snapshot,
                                auth_http: &auth_http,
                                meta: &response_meta,
                                status: *status,
                                headers: headers.as_ref(),
                                auth_attempt: &auth_attempt.summary,
                            })
                            .await?
                        {
                            AuthRejectionOutcome::Retry => {
                                if !is_replayable {
                                    return Err(ApiClientError::Auth {
                                        ctx: ctx.clone(),
                                        source: AuthError::new(
                                            AuthErrorKind::ProviderRejected,
                                            "auth challenge rejected",
                                        ),
                                    });
                                }
                                if auth_retry_index >= max_auth_retries {
                                    return Err(ApiClientError::Auth {
                                        ctx: ctx.clone(),
                                        source: AuthError::new(
                                            AuthErrorKind::ProviderRejected,
                                            format!(
                                                "auth retry budget exhausted (max_auth_retries={max_auth_retries})"
                                            ),
                                        ),
                                    });
                                }
                                auth_retry_index = next_attempt_counter(auth_retry_index, &ctx)?;
                                attempt_index = next_attempt_counter(attempt_index, &ctx)?;
                                continue;
                            }
                            AuthRejectionOutcome::Terminal => {
                                return Err(ApiClientError::Auth {
                                    ctx: ctx.clone(),
                                    source: AuthError::new(
                                        AuthErrorKind::ProviderRejected,
                                        "auth challenge rejected",
                                    ),
                                });
                            }
                            AuthRejectionOutcome::NotProtected => {}
                        }
                    }
                    if !is_replayable {
                        return Err(err);
                    }
                    let outcome = Self::retry_outcome_from_error(&err);
                    let response_headers = Self::retry_response_headers_from_error(&err);
                    let retry_ctx = RetryContext {
                        endpoint: plan.endpoint.meta.name,
                        method: &plan.endpoint.meta.method,
                        url: &url_str,
                        attempt: current_attempt,
                        retry_count: transport_retry_index,
                        page_index: plan.overrides.page_index,
                        idempotent: plan.endpoint.meta.idempotent,
                        max_delay: self.runtime_state.max_retry_delay(),
                        request_headers: &retry_request_headers,
                        response_headers,
                        outcome,
                    };
                    let Some(delay) =
                        self.decide_retry(&err, &retry_config, &retry_ctx, transport_retry_index)?
                    else {
                        return Err(err);
                    };
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    transport_retry_index = next_attempt_counter(transport_retry_index, &ctx)?;
                    attempt_index = next_attempt_counter(attempt_index, &ctx)?;
                }
            }
        }
    }

    pub async fn execute_plan<C>(
        &self,
        plan: RequestPlan,
    ) -> Result<DecodedResponse<C::Value>, ApiClientError>
    where
        C: crate::codec::ResponseCodec,
    {
        let RequestPlan {
            endpoint,
            mut args,
            overrides,
            replayability,
        } = plan;
        let plan = crate::endpoint::RequestPlanView {
            endpoint,
            overrides,
            replayability,
        };
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let dbg = plan.overrides.debug_level.unwrap_or_else(|| self.debug_level());
        let resp = match self
            .drive_attempts(
                &plan,
                &mut args,
                ctx.clone(),
                dbg,
                AttemptFamily::Buffered {
                    skip_body: plan.endpoint.response.no_content,
                },
            )
            .await?
        {
            AttemptTransportSuccess::Buffered(resp) => resp,
            _ => unreachable!(),
        };
        self.maybe_capture_dev_response_body(&plan, &resp);
        self.debug_planned_response(dbg, &resp, resp.url.as_str());
        Self::decode_planned_response::<C>(&plan, resp, ctx.clone())
    }
    pub async fn execute_plan_raw(
        &self,
        plan: RequestPlan,
    ) -> Result<BuiltResponse, ApiClientError> {
        self.execute_plan_raw_with_body(plan, false).await
    }

    pub(crate) async fn execute_plan_raw_skip_body(
        &self,
        plan: RequestPlan,
    ) -> Result<BuiltResponse, ApiClientError> {
        self.execute_plan_raw_with_body(plan, true).await
    }

    async fn execute_plan_raw_with_body(
        &self,
        plan: RequestPlan,
        skip_body: bool,
    ) -> Result<BuiltResponse, ApiClientError> {
        let RequestPlan {
            endpoint,
            mut args,
            overrides,
            replayability,
        } = plan;
        let plan = crate::endpoint::RequestPlanView {
            endpoint,
            overrides,
            replayability,
        };
        let dbg = plan.overrides.debug_level.unwrap_or_else(|| self.debug_level());
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let resp = match self
            .drive_attempts(&plan, &mut args, ctx, dbg, AttemptFamily::Buffered { skip_body })
            .await?
        {
            AttemptTransportSuccess::Buffered(resp) => resp,
            _ => unreachable!(),
        };
        self.maybe_capture_dev_response_body(&plan, &resp);
        self.debug_planned_response(dbg, &resp, resp.url.as_str());
        Ok(resp)
    }
    pub(crate) async fn execute_stream_response<M>(
        &self,
        plan: RequestPlan,
    ) -> Result<crate::stream_response::StreamResponse<M>, ApiClientError>
    where
        M: crate::codec::ContentType,
    {
        let RequestPlan {
            mut endpoint,
            mut args,
            overrides,
            replayability,
        } = plan;
        let ctx = ErrorContext {
            endpoint: endpoint.meta.name,
            method: endpoint.meta.method.clone(),
        };
        if endpoint.response.accept.is_none() {
            endpoint.response.accept = Some(
                <M as crate::codec::ContentType>::try_header_value()
                    .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
            );
        }
        let plan = crate::endpoint::RequestPlanView {
            endpoint,
            overrides,
            replayability,
        };
        if plan.endpoint.pagination.is_some() {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "stream responses do not support pagination".into(),
            });
        }
        if plan.endpoint.response.no_content {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "stream responses cannot use a no-content response plan".into(),
            });
        }
        let dbg = plan.overrides.debug_level.unwrap_or_else(|| self.debug_level());
        let stream_response_limit = self.runtime_state.max_stream_response_body_bytes();
        let resp = match self
            .drive_attempts(
                &plan,
                &mut args,
                ctx.clone(),
                dbg,
                AttemptFamily::Stream {
                    response_limit: stream_response_limit,
                },
            )
            .await?
        {
            AttemptTransportSuccess::Transport(resp) => resp,
            _ => unreachable!(),
        };
        if !Self::header_matches_media_type(resp.headers.get(CONTENT_TYPE), M::CONTENT_TYPE) {
            return Err(ApiClientError::response_contract(
                ctx,
                "stream response content type did not match expected media type",
            ));
        }
        Ok(crate::stream_response::StreamResponse::new(resp, stream_response_limit))
    }
    pub(crate) async fn execute_sse_response<Item, Codec>(
        &self,
        plan: RequestPlan,
    ) -> Result<crate::sse::SseStream<Item>, ApiClientError>
    where
        Item: Send + 'static,
        Codec: crate::sse::SseCodec<Item>,
    {
        let RequestPlan {
            mut endpoint,
            mut args,
            overrides,
            replayability,
        } = plan;
        let ctx = ErrorContext {
            endpoint: endpoint.meta.name,
            method: endpoint.meta.method.clone(),
        };
        if endpoint.response.accept.is_none() {
            endpoint.response.accept = Some(
                <crate::media::EventStream as crate::codec::ContentType>::try_header_value()
                    .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
            );
        }
        let plan = crate::endpoint::RequestPlanView {
            endpoint,
            overrides,
            replayability,
        };
        if plan.endpoint.pagination.is_some() {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "sse responses do not support pagination".into(),
            });
        }
        if plan.endpoint.response.no_content {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "sse responses cannot use a no-content response plan".into(),
            });
        }
        let dbg = plan.overrides.debug_level.unwrap_or_else(|| self.debug_level());
        let response_limit = self.runtime_state.max_stream_response_body_bytes();
        let resp = match self
            .drive_attempts(
                &plan,
                &mut args,
                ctx.clone(),
                dbg,
                AttemptFamily::Sse { response_limit },
            )
            .await?
        {
            AttemptTransportSuccess::Transport(resp) => resp,
            _ => unreachable!(),
        };
        if !Self::header_matches_media_type(resp.headers.get(CONTENT_TYPE), "text/event-stream") {
            return Err(ApiClientError::response_contract(
                ctx,
                "sse response content type did not match expected media type",
            ));
        }
        Ok(SseStream::new(resp, response_limit, Codec::decode_event))
    }
    pub(crate) async fn execute_record_response<Item, F>(
        &self,
        plan: RequestPlan,
    ) -> Result<crate::record::RecordStream<Item>, ApiClientError>
    where
        Item: Send + 'static,
        F: crate::record::RecordFormat<Item>,
    {
        let stream = self.execute_stream_response::<F>(plan).await?;
        Ok(crate::record::RecordStream::new(
            stream.into_transport_response(),
            F::decoder(),
        ))
    }

    pub(crate) async fn execute_multipart_response<PartT, Fmt>(
        &self,
        plan: RequestPlan,
    ) -> Result<crate::multipart_response::MultipartStream<PartT>, ApiClientError>
    where
        PartT: crate::multipart_response::MultipartDecodePart<Fmt>,
        Fmt: crate::multipart::MultipartFormat,
    {
        let RequestPlan {
            mut endpoint,
            mut args,
            overrides,
            replayability,
        } = plan;
        let ctx = ErrorContext {
            endpoint: endpoint.meta.name,
            method: endpoint.meta.method.clone(),
        };
        if endpoint.response.accept.is_none() {
            endpoint.response.accept = Some(
                <Fmt as crate::codec::ContentType>::try_header_value()
                    .map_err(|_| ApiClientError::invalid_param(ctx.clone(), "content_type"))?,
            );
        }
        let plan = crate::endpoint::RequestPlanView {
            endpoint,
            overrides,
            replayability,
        };
        if plan.endpoint.pagination.is_some() {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "multipart responses do not support pagination".into(),
            });
        }
        if plan.endpoint.response.no_content {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "multipart responses cannot use a no-content response plan".into(),
            });
        }
        let dbg = plan.overrides.debug_level.unwrap_or_else(|| self.debug_level());
        let response_limit = self.runtime_state.max_stream_response_body_bytes();
        let resp = match self
            .drive_attempts(
                &plan,
                &mut args,
                ctx.clone(),
                dbg,
                AttemptFamily::Multipart { response_limit },
            )
            .await?
        {
            AttemptTransportSuccess::Transport(resp) => resp,
            _ => unreachable!(),
        };
        let boundary = crate::multipart_response::parse_response_boundary::<Fmt>(
            &resp.headers,
            ctx.clone(),
        )?;
        if let (Some(limit), Some(actual)) = (response_limit, resp.content_length) {
            if actual > limit as u64 {
                return Err(ApiClientError::ResponseTooLarge {
                    ctx: ctx.clone(),
                    limit,
                    actual,
                });
            }
        }
        Ok(crate::multipart_response::MultipartStream::new(
            resp,
            boundary,
            response_limit,
            PartT::decode_headers,
            PartT::decode_part,
        ))
    }
    async fn prepare_auth_plan(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        auth_state: &Cx::AuthState,
        executor: &dyn AuthHttpExecutor,
        built: &mut BuiltRequest,
    ) -> Result<AuthPreparation, ApiClientError> {
        let mut summary = crate::auth::AuthAttemptSummary::default();
        let mut materials = Vec::new();
        for requirement in &plan.endpoint.policy.auth.requirements {
            let auth_meta = built.meta.clone();
            let mut auth_request =
                crate::auth::AuthApplicationRequest::new(&mut built.extensions);
            let prepared = Cx::prepare_auth_requirement(
                requirement,
                &mut auth_request,
                self.vars(),
                self.auth_vars(),
                auth_state,
                executor,
                &auth_meta,
            )
            .await
            .map_err(|source| ApiClientError::Auth {
                ctx: ErrorContext {
                    endpoint: built.meta.endpoint,
                    method: built.meta.method.clone(),
                },
                source,
            })?;
            attach_prepared_auth_generation(built, &prepared);
            let applied = prepared.applied;
            summary.applied.push(applied);
            materials.push(prepared.material);
        }
        Ok(AuthPreparation { summary, materials })
    }

    async fn auth_retry_requested(
        &self,
        ctx: AuthRejectionCtx<'_, Cx, T>,
    ) -> Result<bool, ApiClientError> {
        for applied in &ctx.auth_attempt.applied {
            let Some(requirement) = ctx.plan.endpoint.policy.auth.requirements.iter().find(|req| {
                req.credential.id == applied.credential_id && req.step_id == applied.step_id
            }) else {
                continue;
            };
            match Cx::handle_auth_response(
                requirement,
                applied,
                self.vars(),
                self.auth_vars(),
                ctx.auth_state,
                ctx.auth_http,
                ctx.meta,
                ctx.status,
                ctx.headers,
            )
            .await
            .map_err(|source| ApiClientError::Auth {
                ctx: ErrorContext {
                    endpoint: ctx.meta.endpoint,
                    method: ctx.meta.method.clone(),
                },
                source,
            })? {
                AuthDecision::Continue => {}
                AuthDecision::RetryAfterRefresh { .. } => return Ok(true),
                AuthDecision::Fail => {
                    return Err(ApiClientError::Auth {
                        ctx: ErrorContext {
                            endpoint: ctx.meta.endpoint,
                            method: ctx.meta.method.clone(),
                        },
                        source: AuthError::new(AuthErrorKind::ProviderRejected, "auth challenge rejected"),
                    });
                }
            }
        }
        Ok(false)
    }

    fn decode_planned_response<C>(
        plan: &crate::endpoint::RequestPlanView,
        resp: BuiltResponse,
        ctx: ErrorContext,
    ) -> Result<DecodedResponse<C::Value>, ApiClientError>
    where
        C: crate::codec::ResponseCodec,
    {
        let no_content = plan.endpoint.response.no_content || C::is_no_content();
        if resp.meta.method == http::Method::HEAD && !no_content {
            return Err(ApiClientError::HeadRequiresNoContent { ctx });
        }
        if matches!(resp.status, StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT)
            && !no_content
        {
            return Err(ApiClientError::NoContentStatusRequiresNoContent {
                ctx: ctx.clone(),
                status: resp.status,
            });
        }
        let content_type = resp
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let value = C::decode(
            resp.body.clone(),
            crate::codec::DecodeContext::new(
                ctx.endpoint,
                &ctx.method,
                resp.status,
                content_type,
            ),
        )
        .map_err(|_| {
            ApiClientError::response_body_decode_error(ctx.clone(), resp.status, content_type)
        })?;
        Ok(DecodedResponse {
            meta: resp.meta,
            url: resp.url,
            status: resp.status,
            headers: resp.headers,
            value,
        })
    }

    fn debug_planned_request(&self, dbg: DebugLevel, plan: &crate::endpoint::RequestPlanView, built: &BuiltRequest, url_str: &str) {
        if dbg.is_verbose() {
            self.debug_sink.request_start(
                dbg,
                &plan.endpoint.meta.method,
                url_str,
                built.meta.endpoint,
                built.meta.page_index,
            );
        }
        if dbg.is_very_verbose() {
            self.debug_sink
                .request_headers(dbg, crate::debug::SanitizedHeaders::new(&built.headers));
        }
    }

    fn debug_planned_response(&self, dbg: DebugLevel, resp: &BuiltResponse, url_str: &str) {
        if dbg.is_verbose() {
            self.debug_sink.response_status(dbg, resp.status, url_str, true);
        }
        if dbg.is_very_verbose() {
            self.debug_sink
                .response_headers(dbg, crate::debug::SanitizedHeaders::new(&resp.headers));
        }
    }

    #[allow(deprecated)]
    fn maybe_capture_dev_response_body(&self, plan: &crate::endpoint::RequestPlanView, resp: &BuiltResponse) {
        let Some(capture) = self.runtime_state.dev_body_capture() else {
            return;
        };
        if !plan.endpoint.policy.auth.requirements.is_empty() {
            return;
        }
        capture.capture_response(
            plan.endpoint.meta.name,
            &plan.endpoint.meta.method,
            resp.status,
            &resp.body,
        );
    }
}

fn attach_prepared_auth_generation(
    request: &mut BuiltRequest,
    prepared: &crate::auth::PreparedAuthCredential,
) {
    let slot_id = prepared.material.slot_id();
    for slot in &mut request.extensions.pending_auth_slots {
        if slot.id == slot_id {
            slot.generation = prepared.applied.generation;
            break;
        }
    }
}

fn checked_attempt(
    base_attempt: u32,
    attempt_index: u32,
    ctx: &ErrorContext,
) -> Result<u32, ApiClientError> {
    base_attempt.checked_add(attempt_index).ok_or_else(|| {
        ApiClientError::PolicyViolation {
            ctx: ctx.clone(),
            msg: "request attempt counter overflowed",
        }
    })
}

fn next_attempt_counter(attempt: u32, ctx: &ErrorContext) -> Result<u32, ApiClientError> {
    attempt.checked_add(1).ok_or_else(|| {
        ApiClientError::PolicyViolation {
            ctx: ctx.clone(),
            msg: "request attempt counter overflowed",
        }
    })
}

#[cfg(test)]
mod attempt_counter_tests {
    use super::*;

    #[test]
    fn request_attempt_counter_overflow_returns_error() {
        let ctx = ErrorContext {
            endpoint: "Overflow",
            method: http::Method::GET,
        };
        let err = next_attempt_counter(u32::MAX, &ctx)
            .expect_err("overflowing attempt counter should fail");
        assert!(err.to_string().contains("request attempt counter overflowed"));

        let err = checked_attempt(u32::MAX, 1, &ctx)
            .expect_err("overflowing base plus attempt should fail");
        assert!(err.to_string().contains("request attempt counter overflowed"));
    }
}
