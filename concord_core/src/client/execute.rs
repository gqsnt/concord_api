impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    pub(crate) async fn execute_decoded_ref_with<E, F>(
        &self,
        ep: &E,
        meta: RequestMeta,
        dbg: DebugLevel,
        cache_mode: CacheRequestMode,
        patch_policy: F,
    ) -> Result<DecodedResponse<<E::Response as ResponseSpec>::Output>, ApiClientError>
    where
        E: Endpoint<Cx>,
        F: for<'a> Fn(&mut PolicyPatch<'a>) -> Result<(), ApiClientError>,
    {
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();
        let ctx = Self::ctx_for::<E>(ep);
        let base_attempt = meta.attempt;
        let max_auth_retries = self.runtime_state.max_auth_retries();
        let auth_state_snapshot = self.auth_state();
        let auth_ctrl = <E::Auth as AuthPart<Cx, E>>::controller(
            AuthBuildContext {
                vars: self.vars(),
                auth: self.auth_vars(),
                auth_state: auth_state_snapshot.as_ref(),
            },
            ep,
        )?;
        let mut endpoint_auth_state = auth_ctrl.init(ep)?;
        let mut attempt_index: u32 = 0;
        let mut transport_retry_index: u32 = 0;
        let mut auth_retry_index: u32 = 0;
        let mut force_cache_refresh_for_next_attempt = false;
        let mut used_revalidation_refetch = false;

        loop {
            let mut attempt_meta = meta.clone();
            attempt_meta.attempt = base_attempt.saturating_add(attempt_index);

            let mut built = self.build_request::<E, F>(ep, attempt_meta, &patch_policy)?;
            built.cache_mode = if force_cache_refresh_for_next_attempt {
                force_cache_refresh_for_next_attempt = false;
                CacheRequestMode::Refresh
            } else {
                cache_mode
            };
            let auth_http = ClientAuthHttpExecutor { client: self };
            let auth_request_meta = built.meta.clone();
            let auth_attempt = auth_ctrl
                .prepare(
                    &mut endpoint_auth_state,
                    EndpointAuthPrepareContext {
                        ep,
                        vars: self.vars(),
                        auth: self.auth_vars(),
                        auth_state: auth_state_snapshot.as_ref(),
                        executor: &auth_http,
                        meta: &auth_request_meta,
                        request: &mut built,
                    },
                )
                .await?;
            let url_str = built.url.as_str().to_string();
            let cache_revalidation = match self.prepare_cache_before_request(&mut built).await {
                CacheBeforeOutcome::Hit(cached) => {
                    return Self::decode_built_response::<E>(cached, ctx.clone());
                }
                CacheBeforeOutcome::Continue(cache_revalidation) => cache_revalidation,
            };

            self.debug_request::<E>(dbg, &built, &url_str);
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
                    let auth_action = auth_ctrl
                        .on_response(
                            &mut endpoint_auth_state,
                            EndpointAuthResponseContext {
                                ep,
                                vars: self.vars(),
                                auth: self.auth_vars(),
                                auth_state: auth_state_snapshot.as_ref(),
                                executor: &auth_http,
                                meta: &resp.meta,
                                status: resp.status,
                                headers: &resp.headers,
                                attempt: &auth_attempt,
                            },
                        )
                        .await?;
                    if matches!(auth_action, AuthResponseAction::Retry { .. }) {
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
                    self.debug_response::<E>(dbg, &resp, &url_str);
                    return Self::decode_built_response::<E>(resp, ctx.clone());
                }
                Err(err) => {
                    if let ApiClientError::HttpStatus {
                        status, headers, ..
                    } = &err
                    {
                        let response_meta = RequestMeta {
                            endpoint: ep.name(),
                            method: E::METHOD.clone(),
                            idempotent: meta.idempotent,
                            attempt: base_attempt.saturating_add(attempt_index),
                            page_index: meta.page_index,
                        };
                        let auth_action = auth_ctrl
                            .on_response(
                                &mut endpoint_auth_state,
                                EndpointAuthResponseContext {
                                    ep,
                                    vars: self.vars(),
                                    auth: self.auth_vars(),
                                    auth_state: auth_state_snapshot.as_ref(),
                                    executor: &auth_http,
                                    meta: &response_meta,
                                    status: *status,
                                    headers: headers.as_ref(),
                                    attempt: &auth_attempt,
                                },
                            )
                            .await?;
                        if matches!(auth_action, AuthResponseAction::Retry { .. }) {
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
                        endpoint: ep.name(),
                        method: &E::METHOD,
                        url: &url_str,
                        attempt: base_attempt.saturating_add(attempt_index),
                        retry_count: transport_retry_index,
                        page_index: meta.page_index,
                        idempotent: meta.idempotent,
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
                            return Self::decode_built_response::<E>(cached, ctx.clone());
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

    fn debug_request<E>(&self, dbg: DebugLevel, built: &BuiltRequest, url_str: &str)
    where
        E: Endpoint<Cx>,
    {
        if dbg.is_verbose() {
            self.debug_sink.request_start(
                dbg,
                &E::METHOD,
                url_str,
                built.meta.endpoint,
                built.meta.page_index,
            );
        }

        if dbg.is_very_verbose() {
            self.debug_sink.request_headers(dbg, &built.headers);
            if let Some(body) = built.body.as_ref() {
                const MAX_CHARS: usize = 32 * 1024;
                let fmt = <<E::Body as BodyPart<E>>::Enc as FormatType>::FORMAT_TYPE;
                self.debug_sink.request_body(dbg, body, fmt, MAX_CHARS);
            }
        }
    }

    fn debug_response<E>(&self, dbg: DebugLevel, resp: &BuiltResponse, url_str: &str)
    where
        E: Endpoint<Cx>,
    {
        if dbg.is_verbose() {
            self.debug_sink
                .response_status(dbg, resp.status, url_str, true);
        }
        if dbg.is_very_verbose() {
            const MAX_CHARS: usize = 32 * 1024;
            let fmt = <<E::Response as ResponseSpec>::Dec as FormatType>::FORMAT_TYPE;
            self.debug_sink.response_headers(dbg, &resp.headers);
            self.debug_sink
                .response_body(dbg, &resp.body, fmt, MAX_CHARS);
        }
    }
}

