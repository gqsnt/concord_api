impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub async fn execute_plan<R>(
        &self,
        plan: RequestPlan,
    ) -> Result<DecodedResponse<R>, ApiClientError>
    where
        R: Send + 'static,
    {
        let dbg = self.debug_level();
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
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
            let mut built = self.build_request_from_plan(&plan, meta, cache_mode)?;
            let auth_attempt = self
                .prepare_auth_plan(&plan, &auth_state_snapshot, &auth_http, &mut built)
                .await?;
            let url_str = built.url.as_str().to_string();
            let cache_revalidation = match self.prepare_cache_before_request(&mut built).await {
                CacheBeforeOutcome::Hit(cached) => {
                    return Self::decode_planned_response::<R>(&plan, cached, ctx.clone());
                }
                CacheBeforeOutcome::Continue(cache_revalidation) => cache_revalidation,
            };

            self.debug_planned_request(dbg, &plan, &built, &url_str);
            let inflight_key = self.runtime_state.inflight_policy().key_for(&built);
            let retry_config = built.retry.clone();
            let retry_request_headers = built.headers.clone();
            let built_for_cache = built.clone();
            let send_result = self
                .send_and_classify_with_inflight(
                    built,
                    inflight_key,
                    SendClassifyCtx {
                        dbg,
                        dbg_verbose,
                        dbg_vv,
                        url_str: &url_str,
                        error_ctx: &ctx,
                    },
                )
                .await;

            match send_result {
                Ok(resp) => {
                    if self
                        .auth_retry_requested(
                            &plan,
                            &auth_state_snapshot,
                            &auth_http,
                            &resp.meta,
                            resp.status,
                            &resp.headers,
                            &auth_attempt,
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
                        .apply_cache_after_response(
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
                    return Self::decode_planned_response::<R>(&plan, resp, ctx.clone());
                }
                Err(err) => {
                    if let ApiClientError::HttpStatus { status, headers, .. } = &err {
                        let response_meta = plan
                            .endpoint
                            .meta
                            .request_meta(base_attempt.saturating_add(attempt_index), plan.overrides.page_index);
                        if self
                            .auth_retry_requested(
                                &plan,
                                &auth_state_snapshot,
                                &auth_http,
                                &response_meta,
                                *status,
                                headers.as_ref(),
                                &auth_attempt,
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
                    let Some(mut delay) =
                        self.retry_delay(&retry_config, &retry_ctx, transport_retry_index)
                    else {
                        if built_for_cache.cache_mode == CacheRequestMode::Default
                            && let Some(cached) = self
                                .runtime_state
                                .cache_store()
                                .after_error(&built_for_cache, &err, cache_revalidation.clone())
                                .await
                        {
                            return Self::decode_planned_response::<R>(&plan, cached, ctx.clone());
                        }
                        return Err(err);
                    };
                    if Self::rate_limit_action_from_error(&err)
                        .is_some_and(|action| action.delay_handled_by_rate_limiter())
                    {
                        delay = Duration::ZERO;
                    }
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
    ) -> Result<crate::auth::AuthAttemptSummary, ApiClientError> {
        let mut summary = crate::auth::AuthAttemptSummary::default();
        for requirement in &plan.endpoint.policy.auth.requirements {
            let auth_meta = built.meta.clone();
            let applied = Cx::prepare_auth_requirement(
                requirement,
                built,
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
            built
                .extensions
                .auth_identities
                .push(applied.identity.safe_fragment());
            summary.applied.push(applied);
        }
        Ok(summary)
    }

    #[allow(clippy::too_many_arguments)]
    async fn auth_retry_requested(
        &self,
        plan: &RequestPlan,
        auth_state: &Cx::AuthState,
        executor: &dyn AuthHttpExecutor,
        meta: &RequestMeta,
        status: StatusCode,
        headers: &http::HeaderMap,
        attempt: &crate::auth::AuthAttemptSummary,
    ) -> Result<bool, ApiClientError> {
        for applied in &attempt.applied {
            let Some(requirement) = plan.endpoint.policy.auth.requirements.iter().find(|req| {
                req.credential.id == applied.credential_id && req.step_id == applied.step_id
            }) else {
                continue;
            };
            match Cx::handle_auth_response(
                requirement,
                applied,
                self.vars(),
                self.auth_vars(),
                auth_state,
                executor,
                meta,
                status,
                headers,
            )
            .await
            .map_err(|source| ApiClientError::Auth {
                ctx: ErrorContext {
                    endpoint: meta.endpoint,
                    method: meta.method.clone(),
                },
                source,
            })? {
                AuthDecision::Continue => {}
                AuthDecision::RetryAfterRefresh { .. } => return Ok(true),
                AuthDecision::Fail => {
                    return Err(ApiClientError::Auth {
                        ctx: ErrorContext {
                            endpoint: meta.endpoint,
                            method: meta.method.clone(),
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
            if let Some(body) = built.body.as_ref() {
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
            self.debug_sink
                .response_body(dbg, &resp.body, plan.endpoint.response.format, MAX_CHARS);
        }
    }
}
