impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_attempt_request(
        &self,
        plan: &RequestPlan,
        meta: RequestMeta,
        cache_mode: CacheRequestMode,
    ) -> Result<BuiltRequest, ApiClientError> {
        self.build_request_from_plan(plan, meta, cache_mode)
    }

    async fn prepare_auth(
        &self,
        plan: &RequestPlan,
        auth_state: &Cx::AuthState,
        executor: &dyn AuthHttpExecutor,
        built: &mut BuiltRequest,
    ) -> Result<AuthPreparation, ApiClientError> {
        self.prepare_auth_plan(plan, auth_state, executor, built).await
    }

    async fn check_fresh_cache(&self, built: &mut BuiltRequest) -> CacheBeforeOutcome {
        self.prepare_cache_before_request(built).await
    }

    async fn send_or_join_inflight(
        &self,
        built: BuiltRequest,
        inflight_key: Option<RequestKey>,
        send_ctx: SendClassifyCtx<'_>,
    ) -> Result<BuiltResponse, ApiClientError> {
        self.send_and_classify_with_inflight(built, inflight_key, send_ctx)
            .await
    }

    async fn handle_auth_rejection(
        &self,
        ctx: AuthRejectionCtx<'_, Cx, T>,
    ) -> Result<bool, ApiClientError> {
        self.auth_retry_requested(ctx).await
    }

    fn decide_retry(
        &self,
        err: &ApiClientError,
        retry_config: &RetrySetting,
        retry_ctx: &RetryContext<'_>,
        retry_count: u32,
    ) -> Option<Duration> {
        let mut delay = self.retry_delay(retry_config, retry_ctx, retry_count)?;
        if Self::rate_limit_action_from_error(err)
            .is_some_and(|action| action.delay_handled_by_rate_limiter())
        {
            delay = Duration::ZERO;
        }
        Some(delay)
    }

    async fn maybe_serve_stale(
        &self,
        built_for_cache: &BuiltRequest,
        err: &ApiClientError,
        cache_revalidation: Option<CacheRevalidation>,
        dbg: DebugLevel,
        url_str: &str,
    ) -> Result<Option<BuiltResponse>, ApiClientError> {
        if built_for_cache.cache_mode != CacheRequestMode::Default {
            return Ok(None);
        }
        let Some(cached) = self
            .runtime_state
            .cache_store()
            .after_error(built_for_cache, err, cache_revalidation)
            .await
        else {
            return Ok(None);
        };
        if dbg.is_verbose() {
            self.debug_sink.stale_fallback(
                dbg,
                &built_for_cache.meta.method,
                url_str,
                built_for_cache.meta.endpoint,
                built_for_cache.meta.page_index,
            );
        }
        Ok(Some(cached))
    }

    async fn maybe_write_cache(
        &self,
        built_for_cache: &BuiltRequest,
        resp: BuiltResponse,
        cache_revalidation: Option<CacheRevalidation>,
        used_revalidation_refetch: bool,
    ) -> CacheAfterOutcome {
        self.apply_cache_after_response(
            built_for_cache,
            resp,
            cache_revalidation,
            used_revalidation_refetch,
        )
        .await
    }

    pub async fn execute_plan<R>(
        &self,
        plan: RequestPlan,
    ) -> Result<DecodedResponse<R>, ApiClientError>
    where
        R: Send + 'static,
    {
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let resp = self.execute_plan_raw(plan.clone()).await?;
        Self::decode_planned_response::<R>(&plan, resp, ctx)
    }

    pub async fn execute_plan_raw(
        &self,
        plan: RequestPlan,
    ) -> Result<BuiltResponse, ApiClientError> {
        let dbg = plan.overrides.debug_level.unwrap_or_else(|| self.debug_level());
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        if let RetrySetting::Config(config) = &plan.endpoint.policy.retry {
            config.validate(ctx.clone())?;
        }
        let base_attempt: u32 = plan.overrides.attempt;
        let max_auth_retries = self.runtime_state.max_auth_retries();
        let auth_state_snapshot = self.auth_state();
        let auth_http = ClientAuthHttpExecutor { client: self };
        let mut attempt_index: u32 = 0;
        let mut transport_retry_index: u32 = 0;
        let mut auth_retry_index: u32 = 0;
        let mut force_cache_refresh_for_next_attempt = false;
        let mut used_revalidation_refetch = false;

        loop {
            let meta = plan.endpoint.meta.request_meta(base_attempt.saturating_add(attempt_index), plan.overrides.page_index);
            let cache_mode = if force_cache_refresh_for_next_attempt {
                force_cache_refresh_for_next_attempt = false;
                CacheRequestMode::Refresh
            } else {
                plan.overrides.cache_mode
            };
            let mut built = self.build_attempt_request(&plan, meta, cache_mode)?;
            let auth_attempt = self
                .prepare_auth(&plan, &auth_state_snapshot, &auth_http, &mut built)
                .await?;
            let url_str = built.debug_url();
            let cache_revalidation = match self.check_fresh_cache(&mut built).await {
                CacheBeforeOutcome::Hit(cached) => {
                    return Ok(cached);
                }
                CacheBeforeOutcome::Continue(cache_revalidation) => cache_revalidation,
            };

            self.debug_planned_request(dbg, &plan, &built, &url_str);
            let inflight_key = self.runtime_state.inflight_policy().key_for(&built);
            let retry_config = built.retry.clone();
            let retry_request_headers = built.headers.clone();
            let built_for_cache = built.clone();
            let send_result = self
                .send_or_join_inflight(
                    built,
                    inflight_key,
                    SendClassifyCtx {
                        dbg,
                        dbg_verbose,
                        dbg_vv,
                        url_str: &url_str,
                        error_ctx: &ctx,
                        auth_materials: &auth_attempt.materials,
                    },
                )
                .await;

            match send_result {
                Ok(resp) => {
                    if self
                        .handle_auth_rejection(
                            AuthRejectionCtx {
                                plan: &plan,
                                auth_state: &auth_state_snapshot,
                                auth_http: &auth_http,
                                meta: &resp.meta,
                                status: resp.status,
                                headers: &resp.headers,
                                auth_attempt: &auth_attempt.summary,
                            },
                        )
                        .await?
                    {
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
                        auth_retry_index = auth_retry_index.saturating_add(1);
                        attempt_index = attempt_index.saturating_add(1);
                        continue;
                    }
                    let cache_after = self
                        .maybe_write_cache(
                            &built_for_cache,
                            resp,
                            cache_revalidation,
                            used_revalidation_refetch,
                        )
                        .await;
                    if cache_after.needs_revalidation_refetch {
                        used_revalidation_refetch = true;
                        force_cache_refresh_for_next_attempt = true;
                        attempt_index = attempt_index.saturating_add(1);
                        continue;
                    }
                    let resp = cache_after.response;
                    self.debug_planned_response(dbg, &plan, &resp, &url_str);
                    return Ok(resp);
                }
                Err(err) => {
                    if let ApiClientError::HttpStatus { status, headers, .. } = &err {
                        let response_meta = plan
                            .endpoint
                            .meta
                            .request_meta(base_attempt.saturating_add(attempt_index), plan.overrides.page_index);
                        if self
                            .handle_auth_rejection(
                                AuthRejectionCtx {
                                    plan: &plan,
                                    auth_state: &auth_state_snapshot,
                                    auth_http: &auth_http,
                                    meta: &response_meta,
                                    status: *status,
                                    headers: headers.as_ref(),
                                    auth_attempt: &auth_attempt.summary,
                                },
                            )
                            .await?
                        {
                            if auth_retry_index >= max_auth_retries {
                                return Err(err);
                            }
                            auth_retry_index = auth_retry_index.saturating_add(1);
                            attempt_index = attempt_index.saturating_add(1);
                            continue;
                        }
                    }
                    let outcome = Self::retry_outcome_from_error(&err);
                    let response_headers = Self::retry_response_headers_from_error(&err);
                    let retry_ctx = RetryContext {
                        endpoint: plan.endpoint.meta.name,
                        method: &plan.endpoint.meta.method,
                        url: &url_str,
                        attempt: base_attempt.saturating_add(attempt_index),
                        retry_count: transport_retry_index,
                        page_index: plan.overrides.page_index,
                        idempotent: plan.endpoint.meta.idempotent,
                        request_headers: &retry_request_headers,
                        response_headers,
                        outcome,
                    };
                    let Some(delay) = self.decide_retry(
                        &err,
                        &retry_config,
                        &retry_ctx,
                        transport_retry_index,
                    ) else {
                        if let Some(cached) = self
                            .maybe_serve_stale(
                                &built_for_cache,
                                &err,
                                cache_revalidation.clone(),
                                dbg,
                                &url_str,
                            )
                            .await?
                        {
                            return Ok(cached);
                        }
                        return Err(err);
                    };
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    transport_retry_index = transport_retry_index.saturating_add(1);
                    attempt_index = attempt_index.saturating_add(1);
                }
            }
        }
    }

    async fn prepare_auth_plan(
        &self,
        plan: &RequestPlan,
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
            built
                .extensions
                .auth_identities
                .push(applied.identity.safe_fragment());
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

    fn decode_planned_response<R>(
        plan: &RequestPlan,
        resp: BuiltResponse,
        ctx: ErrorContext,
    ) -> Result<DecodedResponse<R>, ApiClientError>
    where
        R: Send + 'static,
    {
        if resp.meta.method == http::Method::HEAD && !plan.endpoint.response.no_content {
            return Err(ApiClientError::HeadRequiresNoContent { ctx });
        }
        if matches!(resp.status, StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT)
            && !plan.endpoint.response.no_content
        {
            return Err(ApiClientError::NoContentStatusRequiresNoContent {
                ctx: ctx.clone(),
                status: resp.status,
            });
        }
        let decoded = (plan.endpoint.response.decode)(resp, ctx.clone())?;
        decoded
            .downcast::<DecodedResponse<R>>()
            .map(|boxed| *boxed)
            .map_err(|_| ApiClientError::Transform {
                ctx,
                source: "planned response decoder returned an unexpected type".into(),
            })
    }

    async fn prepare_cache_before_request(&self, built: &mut BuiltRequest) -> CacheBeforeOutcome {
        match built.cache_mode {
            CacheRequestMode::Default => {
                match self.runtime_state.cache_store().before_request(built).await {
                    CacheBefore::Hit(cached) => CacheBeforeOutcome::Hit(cached),
                    CacheBefore::Revalidate {
                        request_headers,
                        cached,
                    } => {
                        built.headers = request_headers;
                        built.cache_revalidation = Some(cached.clone());
                        CacheBeforeOutcome::Continue(Some(cached))
                    }
                    CacheBefore::Miss | CacheBefore::Bypass => CacheBeforeOutcome::Continue(None),
                }
            }
            CacheRequestMode::Bypass | CacheRequestMode::Refresh => {
                CacheBeforeOutcome::Continue(None)
            }
        }
    }

    async fn apply_cache_after_response(
        &self,
        built_for_cache: &BuiltRequest,
        resp: BuiltResponse,
        cache_revalidation: Option<CacheRevalidation>,
        used_revalidation_refetch: bool,
    ) -> CacheAfterOutcome {
        let was_revalidation_304 =
            resp.status == StatusCode::NOT_MODIFIED && cache_revalidation.is_some();
        let cache_after = if built_for_cache.cache_mode == CacheRequestMode::Bypass {
            None
        } else {
            Some(
                self.runtime_state
                    .cache_store()
                    .after_response(built_for_cache, &resp, cache_revalidation)
                    .await,
            )
        };
        let updated = match cache_after {
            Some(CacheAfter::Updated(updated)) => Some(*updated),
            _ => None,
        };
        let needs_revalidation_refetch =
            was_revalidation_304 && updated.is_none() && !used_revalidation_refetch;
        CacheAfterOutcome {
            response: updated.unwrap_or(resp),
            needs_revalidation_refetch,
        }
    }

    fn debug_planned_request(&self, dbg: DebugLevel, plan: &RequestPlan, built: &BuiltRequest, url_str: &str) {
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
            self.debug_sink.request_headers(dbg, &built.headers);
            if self.debug_body {
                let Some(body) = built.body.as_ref() else {
                    return;
                };
                const MAX_CHARS: usize = 32 * 1024;
                let fmt = match &plan.endpoint.body {
                    BodyPlan::Encoded { format, .. } => *format,
                    BodyPlan::None => crate::codec::Format::Text,
                };
                self.debug_sink.request_body(dbg, body, fmt, MAX_CHARS);
            }
        }
    }

    fn debug_planned_response(&self, dbg: DebugLevel, plan: &RequestPlan, resp: &BuiltResponse, url_str: &str) {
        if dbg.is_verbose() {
            self.debug_sink.response_status(dbg, resp.status, url_str, true);
        }
        if dbg.is_very_verbose() {
            const MAX_CHARS: usize = 32 * 1024;
            self.debug_sink.response_headers(dbg, &resp.headers);
            if self.debug_body {
                self.debug_sink
                    .response_body(dbg, &resp.body, plan.endpoint.response.format, MAX_CHARS);
            }
        }
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
