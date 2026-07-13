// Client lifecycle phase modules intentionally share one private parent namespace.
use super::build::PublicRequestHead;
use super::*;

enum AuthRejectionStep {
    Retry(AdmissionPermit),
    Fail(ApiClientError),
}

#[derive(Clone)]
struct AuthResendIntent {
    rejection_plan: crate::auth::AuthRejectionPlan,
    status: StatusCode,
    response_meta: RequestMeta,
    auth_attempt: crate::auth::AuthAttemptSummary,
    error_ctx: ErrorContext,
}

enum PendingResend {
    Ordinary {
        error: ApiClientError,
        delay: Duration,
    },
    Auth(AuthResendIntent),
}

struct AuthRejectionStepCtx<'a, Cx: ClientContext, T: Transport> {
    plan: &'a crate::endpoint::RequestPlanView,
    auth_state: &'a Cx::AuthState,
    auth_http: &'a ClientAuthHttpExecutor<'a, Cx, T>,
    response_meta: &'a RequestMeta,
    auth_attempt: &'a crate::auth::AuthAttemptSummary,
    rejection_plan: &'a crate::auth::AuthRejectionPlan,
    status: StatusCode,
    error_ctx: &'a ErrorContext,
    is_replayable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttemptFamily {
    Buffered { skip_body: bool },
    Stream { response_limit: Option<usize> },
}

enum AttemptTransportSuccess {
    Buffered(AttemptResponse),
    Transport(AttemptResponse),
}

struct CachedAuthPreparation {
    preparation: AuthPreparation,
}

impl CachedAuthPreparation {
    fn new(preparation: AuthPreparation) -> Self {
        Self { preparation }
    }

    fn preparation(&self) -> &AuthPreparation {
        &self.preparation
    }
}

impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    async fn prepare_auth(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        auth_state: &Cx::AuthState,
        executor: &dyn AuthHttpExecutor,
        head: &PublicRequestHead,
    ) -> Result<AuthPreparation, ApiClientError> {
        self.prepare_auth_plan(plan, auth_state, executor, head)
            .await
    }

    fn classify_auth_rejection(
        &self,
        ctx: AuthRejectionCtx<'_, Cx>,
    ) -> Result<Option<crate::auth::AuthRejectionPlan>, ApiClientError> {
        if !Self::is_protected_auth_rejection(ctx.plan, ctx.status) {
            return Ok(None);
        }
        let mut aggregate = crate::auth::AuthRejectionPlan::default();
        for applied in &ctx.auth_attempt.applied {
            let Some(requirement) = ctx
                .plan
                .endpoint
                .policy
                .auth
                .requirements
                .iter()
                .find(|req| {
                    req.credential.id == applied.credential_id
                        && req.usage_id == applied.usage_id
                        && req.step_id == applied.step_id
                })
            else {
                return Err(Self::auth_plan_mismatch(
                    ctx.meta,
                    "authentication requirement use was not applied",
                ));
            };
            let action = crate::auth::plan_rejection::<Cx>(
                requirement,
                applied,
                self.vars(),
                self.auth_vars(),
                ctx.auth_state,
                ctx.meta,
                ctx.status,
                ctx.headers,
            )
            .map_err(|source| ApiClientError::Auth {
                ctx: ErrorContext {
                    endpoint: ctx.meta.endpoint,
                    method: ctx.meta.method.clone(),
                },
                source,
            })?;
            if !action.matches(requirement, applied) {
                return Err(Self::auth_plan_mismatch(
                    ctx.meta,
                    "authentication rejection action did not match its planning pair",
                ));
            }
            if aggregate
                .actions()
                .iter()
                .any(|existing| existing.matches(requirement, applied))
            {
                return Err(Self::auth_plan_mismatch(
                    ctx.meta,
                    "duplicate authentication rejection action for one applied use",
                ));
            }
            aggregate.append_validated(action);
        }
        if aggregate.actions().is_empty() {
            Ok(None)
        } else {
            Ok(Some(aggregate))
        }
    }

    fn auth_plan_mismatch(meta: &RequestMeta, message: &'static str) -> ApiClientError {
        ApiClientError::Auth {
            ctx: ErrorContext {
                endpoint: meta.endpoint,
                method: meta.method.clone(),
            },
            source: AuthError::new(AuthErrorKind::InvalidConfiguration, message),
        }
    }

    fn is_protected_auth_rejection(
        plan: &crate::endpoint::RequestPlanView,
        status: StatusCode,
    ) -> bool {
        matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            && !plan.endpoint.policy.auth.requirements.is_empty()
    }

    async fn apply_auth_rejection_step(
        &self,
        ctx: AuthRejectionStepCtx<'_, Cx, T>,
        admission: Option<AdmissionPermit>,
    ) -> Result<AuthRejectionStep, ApiClientError> {
        if !ctx.is_replayable && ctx.rejection_plan.requests_refresh() {
            return Ok(AuthRejectionStep::Fail(Self::auth_challenge_rejected(
                ctx.error_ctx,
            )));
        }
        if ctx.rejection_plan.requests_refresh() && admission.is_none() {
            return Ok(AuthRejectionStep::Fail(Self::auth_attempt_cap_exhausted(
                ctx.error_ctx,
            )));
        }
        self.apply_auth_rejection_plan(&ctx).await?;
        if ctx.rejection_plan.requests_refresh() {
            match admission {
                Some(admission) => Ok(AuthRejectionStep::Retry(admission)),
                None => Ok(AuthRejectionStep::Fail(Self::auth_attempt_cap_exhausted(
                    ctx.error_ctx,
                ))),
            }
        } else {
            Ok(AuthRejectionStep::Fail(Self::auth_challenge_rejected(
                ctx.error_ctx,
            )))
        }
    }

    async fn apply_auth_rejection_plan(
        &self,
        ctx: &AuthRejectionStepCtx<'_, Cx, T>,
    ) -> Result<(), ApiClientError> {
        let requirements = &ctx.plan.endpoint.policy.auth.requirements;
        let applied = &ctx.auth_attempt.applied;
        let mut bindings = Vec::with_capacity(ctx.rejection_plan.actions().len());

        // Resolve every action before applying any of them. A stale or
        // ambiguous aggregate therefore cannot partially mutate credentials.
        for (action_index, action) in ctx.rejection_plan.actions().iter().enumerate() {
            let binding = applied
                .iter()
                .enumerate()
                .find_map(|(applied_index, applied)| {
                    requirements
                        .iter()
                        .enumerate()
                        .find(|(_, requirement)| {
                            requirement.credential.id == applied.credential_id
                                && requirement.usage_id == applied.usage_id
                                && requirement.step_id == applied.step_id
                                && action.matches(requirement, applied)
                        })
                        .map(|(requirement_index, _)| {
                            (action_index, requirement_index, applied_index)
                        })
                });
            let Some(binding) = binding else {
                return Err(ApiClientError::Auth {
                    ctx: ErrorContext {
                        endpoint: ctx.response_meta.endpoint,
                        method: ctx.response_meta.method.clone(),
                    },
                    source: AuthError::new(
                        AuthErrorKind::InvalidConfiguration,
                        "authentication rejection plan no longer matches the applied credential",
                    ),
                });
            };
            bindings.push(binding);
        }

        for (action_index, requirement_index, applied_index) in bindings {
            let action = &ctx.rejection_plan.actions()[action_index];
            let requirement = &requirements[requirement_index];
            let applied = &applied[applied_index];
            let result = crate::auth::apply_rejection::<Cx>(
                action,
                requirement,
                applied,
                self.vars(),
                self.auth_vars(),
                ctx.auth_state,
                ctx.auth_http,
                ctx.response_meta,
                ctx.status,
            )
            .await;
            result.map_err(|source| ApiClientError::Auth {
                ctx: ErrorContext {
                    endpoint: ctx.response_meta.endpoint,
                    method: ctx.response_meta.method.clone(),
                },
                source,
            })?;
        }
        Ok(())
    }

    fn auth_challenge_rejected(ctx: &ErrorContext) -> ApiClientError {
        ApiClientError::Auth {
            ctx: ctx.clone(),
            source: AuthError::new(AuthErrorKind::ProviderRejected, "auth challenge rejected"),
        }
    }

    fn auth_attempt_cap_exhausted(ctx: &ErrorContext) -> ApiClientError {
        ApiClientError::Auth {
            ctx: ctx.clone(),
            source: AuthError::new(
                AuthErrorKind::ProviderRejected,
                "maximum request attempts exhausted before authentication refresh",
            ),
        }
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

    fn retry_request_headers_snapshot(
        &self,
        retry_config: &RetrySetting,
        attempts_used: u32,
        max_attempts: u32,
        built: &BuiltRequest,
    ) -> Option<http::HeaderMap> {
        match retry_config {
            RetrySetting::Config(config)
                if attempts_used < max_attempts
                    && matches!(
                        &config.idempotency,
                        crate::retry::RetryIdempotency::Header(_)
                    ) =>
            {
                Some(built.message.headers().clone())
            }
            RetrySetting::Inherit if attempts_used < max_attempts => {
                Some(built.message.headers().clone())
            }
            _ => None,
        }
    }

    async fn drive_attempts(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        body: &mut crate::io::PreparedBody,
        ctx: ErrorContext,
        dbg: DebugLevel,
        family: AttemptFamily,
    ) -> Result<AttemptTransportSuccess, ApiClientError> {
        let dbg_verbose = dbg.is_verbose();
        let dbg_vv = dbg.is_very_verbose();
        let is_replayable = body.is_replayable();
        if let RetrySetting::Config(config) = &plan.endpoint.policy.retry {
            config.validate(ctx.clone())?;
        }
        let max_attempts = match &plan.endpoint.policy.retry {
            RetrySetting::Config(config) => config.max_attempts,
            RetrySetting::Inherit => self.runtime_state.max_attempts(),
            RetrySetting::Off => 1,
        };
        crate::retry::validate_max_attempts(max_attempts, ctx.clone())?;
        let base_attempt: u32 = plan.overrides.attempt;
        let auth_state_snapshot = self
            .try_auth_state()
            .map_err(|source| ApiClientError::Auth {
                ctx: ctx.clone(),
                source,
            })?;
        let auth_http = ClientAuthHttpExecutor { client: self };
        let mut auth_placement_plan: Option<crate::auth::AuthPlacementPlan> = None;
        let mut pending_resend: Option<PendingResend> = None;
        // One authoritative request-local count. It is incremented only at
        // the physical Transport::send invocation in send_flow. Existing
        // RequestMeta/HookMeta attempt values are derived zero-based indexes.
        let mut attempts_used: u32 = 0;
        // Request-local auth preparation cache.
        // It is reused across transport/status retries and cleared when auth
        // response handling asks for a refreshed credential state.
        let mut cached_auth_preparation: Option<CachedAuthPreparation> = None;

        loop {
            let current_attempt = checked_attempt(base_attempt, attempts_used, &ctx)?;
            let meta = plan
                .endpoint
                .meta
                .request_meta(current_attempt, plan.overrides.page_index);
            let mut head = self.resolve_public_request_head(plan, body, meta)?;
            let origin_key =
                OriginKey::from_url(&head.url).map_err(|_| ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "request origin is invalid or unsupported",
                })?;
            let auth_plan = match auth_placement_plan.as_ref() {
                Some(existing) => existing,
                None => auth_placement_plan.insert(
                    crate::auth::AuthPlacementPlan::from_auth_plan(&plan.endpoint.policy.auth)
                        .map_err(|source| ApiClientError::Auth {
                            ctx: ctx.clone(),
                            source,
                        })?,
                ),
            };
            head.apply_auth_preflight(auth_plan, &ctx)?;
            // The handle is scoped to this resolved attempt. A retry whose
            // public request resolves to another origin therefore cannot
            // spend or deposit against the previous origin.
            let origin_handle = self.runtime_state.retry_admission().track(origin_key);
            let origin = &origin_handle;

            let mut attempt_admission = None;
            if let Some(resend) = pending_resend.take() {
                match resend {
                    PendingResend::Ordinary { error, delay } => {
                        if !is_replayable || attempts_used >= max_attempts {
                            return Err(error);
                        }
                        let Some(admission) = origin.reserve() else {
                            return Err(error);
                        };
                        if !delay.is_zero() {
                            tokio::time::sleep(delay).await;
                        }
                        attempt_admission = Some(admission);
                    }
                    PendingResend::Auth(intent) => {
                        let fallback = Self::auth_challenge_rejected(&intent.error_ctx);
                        if !is_replayable || attempts_used >= max_attempts {
                            return Err(fallback);
                        }
                        let Some(admission) = origin.reserve() else {
                            return Err(fallback);
                        };
                        let step = self
                            .apply_auth_rejection_step(
                                AuthRejectionStepCtx {
                                    plan,
                                    auth_state: &auth_state_snapshot,
                                    auth_http: &auth_http,
                                    response_meta: &intent.response_meta,
                                    auth_attempt: &intent.auth_attempt,
                                    rejection_plan: &intent.rejection_plan,
                                    status: intent.status,
                                    error_ctx: &intent.error_ctx,
                                    is_replayable,
                                },
                                Some(admission),
                            )
                            .await?;
                        match step {
                            AuthRejectionStep::Retry(admission) => {
                                cached_auth_preparation = None;
                                attempt_admission = Some(admission);
                            }
                            AuthRejectionStep::Fail(err) => return Err(err),
                        }
                    }
                }
            }
            let auth_preparation = if cached_auth_preparation.is_none() {
                Some(
                    self.prepare_auth(plan, &auth_state_snapshot, &auth_http, &head)
                        .await?,
                )
            } else {
                None
            };
            let auth_attempt = if let Some(cache) = cached_auth_preparation.as_ref() {
                cache.preparation()
            } else {
                let prepared = auth_preparation
                    .as_ref()
                    .expect("prepared auth must exist when cache is absent");
                if prepared.cache_policy.allows_request_local_reuse() {
                    cached_auth_preparation = Some(CachedAuthPreparation::new(prepared.clone()));
                }
                prepared
            };
            let attempt_body = self.produce_attempt_body(body, &ctx)?;
            let mut built = head.finish(attempt_body, &ctx)?;
            let url_str = built.debug_url();

            let retry_config = std::mem::take(&mut built.retry);
            let retry_request_headers = self.retry_request_headers_snapshot(
                &retry_config,
                attempts_used,
                max_attempts,
                &built,
            );
            let send_result = match family {
                AttemptFamily::Buffered { skip_body: _ } => self
                    .send_and_classify_once(
                        built,
                        SendClassifyCtx {
                            dbg,
                            dbg_verbose,
                            dbg_vv,
                            url_str: &url_str,
                            error_ctx: &ctx,
                            auth_materials: &auth_attempt.materials,
                        },
                        &mut attempts_used,
                        attempt_admission.take(),
                        origin,
                    )
                    .await
                    .map(AttemptTransportSuccess::Buffered),
                AttemptFamily::Stream { response_limit: _ } => self
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
                        &mut attempts_used,
                        attempt_admission.take(),
                        origin,
                    )
                    .await
                    .map(AttemptTransportSuccess::Transport),
            };

            match send_result {
                Ok(resp) => {
                    let (response_status, response_meta, response_headers) = match &resp {
                        AttemptTransportSuccess::Buffered(resp) => {
                            (resp.status(), &resp.context.meta, resp.headers())
                        }
                        AttemptTransportSuccess::Transport(resp) => {
                            (resp.status(), &resp.context.meta, resp.headers())
                        }
                    };
                    let classification = self.classify_auth_rejection(AuthRejectionCtx {
                        plan,
                        auth_state: &auth_state_snapshot,
                        meta: response_meta,
                        status: response_status,
                        headers: response_headers,
                        auth_attempt: &auth_attempt.summary,
                    })?;
                    match classification {
                        Some(rejection_plan) if rejection_plan.requests_refresh() => {
                            if !is_replayable {
                                return Err(Self::auth_challenge_rejected(&ctx));
                            }
                            if attempts_used >= max_attempts {
                                return Err(Self::auth_attempt_cap_exhausted(&ctx));
                            }
                            pending_resend = Some(PendingResend::Auth(AuthResendIntent {
                                rejection_plan,
                                status: response_status,
                                response_meta: response_meta.clone(),
                                auth_attempt: auth_attempt.summary.clone(),
                                error_ctx: ctx.clone(),
                            }));
                            continue;
                        }
                        Some(rejection_plan) => match self
                            .apply_auth_rejection_step(
                                AuthRejectionStepCtx {
                                    plan,
                                    auth_state: &auth_state_snapshot,
                                    auth_http: &auth_http,
                                    response_meta,
                                    auth_attempt: &auth_attempt.summary,
                                    rejection_plan: &rejection_plan,
                                    status: response_status,
                                    error_ctx: &ctx,
                                    is_replayable,
                                },
                                None,
                            )
                            .await?
                        {
                            AuthRejectionStep::Fail(err) => return Err(err),
                            AuthRejectionStep::Retry(_) => {
                                return Err(Self::auth_challenge_rejected(&ctx));
                            }
                        },
                        None => {}
                    }
                    let resp = match (family, resp) {
                        (
                            AttemptFamily::Buffered { .. },
                            AttemptTransportSuccess::Buffered(resp),
                        ) => AttemptTransportSuccess::Buffered(resp),
                        (
                            AttemptFamily::Stream { response_limit },
                            AttemptTransportSuccess::Transport(resp),
                        ) => AttemptTransportSuccess::Transport(Self::limit_response_body(
                            resp,
                            response_limit,
                            &ctx,
                        )?),
                        _ => unreachable!("attempt response family changed during classification"),
                    };
                    return Ok(match resp {
                        AttemptTransportSuccess::Buffered(resp) => {
                            AttemptTransportSuccess::Buffered(Self::retain_response_origin(
                                resp,
                                origin_handle,
                            ))
                        }
                        AttemptTransportSuccess::Transport(resp) => {
                            AttemptTransportSuccess::Transport(Self::retain_response_origin(
                                resp,
                                origin_handle,
                            ))
                        }
                    });
                }
                Err(err) => {
                    if matches!(
                        &err,
                        ApiClientError::ResponseTooLarge { .. }
                            | ApiClientError::ResponseBodyLimitExceeded { .. }
                            | ApiClientError::ResponseBody { .. }
                            | ApiClientError::Decode { .. }
                            | ApiClientError::Codec { .. }
                    ) {
                        return Err(err);
                    }
                    if let ApiClientError::HttpStatus {
                        status, headers, ..
                    } = &err
                    {
                        let response_meta = plan
                            .endpoint
                            .meta
                            .request_meta(current_attempt, plan.overrides.page_index);
                        let classification = self.classify_auth_rejection(AuthRejectionCtx {
                            plan,
                            auth_state: &auth_state_snapshot,
                            meta: &response_meta,
                            status: *status,
                            headers: headers.as_ref(),
                            auth_attempt: &auth_attempt.summary,
                        })?;
                        match classification {
                            Some(rejection_plan) if rejection_plan.requests_refresh() => {
                                if !is_replayable {
                                    return Err(Self::auth_challenge_rejected(&ctx));
                                }
                                if attempts_used >= max_attempts {
                                    return Err(Self::auth_attempt_cap_exhausted(&ctx));
                                }
                                pending_resend = Some(PendingResend::Auth(AuthResendIntent {
                                    rejection_plan,
                                    status: *status,
                                    response_meta,
                                    auth_attempt: auth_attempt.summary.clone(),
                                    error_ctx: ctx.clone(),
                                }));
                                continue;
                            }
                            Some(rejection_plan) => match self
                                .apply_auth_rejection_step(
                                    AuthRejectionStepCtx {
                                        plan,
                                        auth_state: &auth_state_snapshot,
                                        auth_http: &auth_http,
                                        response_meta: &response_meta,
                                        auth_attempt: &auth_attempt.summary,
                                        rejection_plan: &rejection_plan,
                                        status: *status,
                                        error_ctx: &ctx,
                                        is_replayable,
                                    },
                                    None,
                                )
                                .await?
                            {
                                AuthRejectionStep::Fail(err) => return Err(err),
                                AuthRejectionStep::Retry(_) => {
                                    return Err(Self::auth_challenge_rejected(&ctx));
                                }
                            },
                            None => {}
                        }
                    }
                    if !is_replayable {
                        return Err(err);
                    }
                    let outcome = Self::retry_outcome_from_error(&err);
                    let response_headers = Self::retry_response_headers_from_error(&err);
                    let empty_request_headers = http::HeaderMap::new();
                    let request_headers = retry_request_headers
                        .as_ref()
                        .unwrap_or(&empty_request_headers);
                    let retry_ctx = RetryContext {
                        endpoint: plan.endpoint.meta.name,
                        method: &plan.endpoint.meta.method,
                        url: &url_str,
                        attempt: current_attempt,
                        retry_count: attempts_used.saturating_sub(1),
                        page_index: plan.overrides.page_index,
                        idempotent: plan.endpoint.meta.idempotent,
                        max_delay: self.runtime_state.max_rate_limit_cooldown(),
                        request_headers,
                        response_headers,
                        outcome,
                    };
                    let Some(delay) =
                        self.decide_retry(&err, &retry_config, &retry_ctx, attempts_used)?
                    else {
                        return Err(err);
                    };
                    // Classification remains observable at the terminal
                    // absolute cap, but it cannot admit another send.
                    if attempts_used >= max_attempts {
                        return Err(err);
                    }
                    pending_resend = Some(PendingResend::Ordinary { error: err, delay });
                }
            }
        }
    }

    fn retain_response_origin(resp: AttemptResponse, origin: OriginHandle) -> AttemptResponse {
        let AttemptResponse { message, context } = resp;
        let (parts, body) = message.into_parts();
        AttemptResponse {
            message: http::Response::from_parts(parts, crate::body::retain_origin(body, origin)),
            context,
        }
    }

    pub async fn execute_plan<C>(
        &self,
        plan: RequestPlan,
    ) -> Result<DecodedResponse<C::Value>, ApiClientError>
    where
        C: crate::codec::ResponseCodec,
    {
        let (plan, mut body) = into_canonical_request_plan_view(plan);
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let dbg = plan
            .overrides
            .debug_level
            .unwrap_or_else(|| self.debug_level());
        let resp = match self
            .drive_attempts(
                &plan,
                &mut body,
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
        let resp = Self::buffer_response(
            resp,
            plan.endpoint.response.no_content,
            self.runtime_state.max_response_body_bytes(),
            &ctx,
        )
        .await?;
        #[cfg(feature = "dangerous-dev-tools")]
        self.maybe_capture_dev_response_body(&plan, &resp);
        self.debug_planned_response(dbg, &resp, resp.url().as_str());
        Self::decode_planned_response::<C>(&plan, resp, ctx.clone())
    }
    pub(crate) async fn execute_plan_raw(
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
        let (plan, mut body) = into_canonical_request_plan_view(plan);
        let dbg = plan
            .overrides
            .debug_level
            .unwrap_or_else(|| self.debug_level());
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let resp = match self
            .drive_attempts(
                &plan,
                &mut body,
                ctx.clone(),
                dbg,
                AttemptFamily::Buffered { skip_body },
            )
            .await?
        {
            AttemptTransportSuccess::Buffered(resp) => resp,
            _ => unreachable!(),
        };
        let resp = Self::buffer_response(
            resp,
            skip_body,
            self.runtime_state.max_response_body_bytes(),
            &ctx,
        )
        .await?;
        #[cfg(feature = "dangerous-dev-tools")]
        self.maybe_capture_dev_response_body(&plan, &resp);
        self.debug_planned_response(dbg, &resp, resp.url().as_str());
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
            body,
            overrides,
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
        let (plan, mut body) = into_canonical_request_plan_view(RequestPlan {
            endpoint,
            body,
            overrides,
        });
        if plan.endpoint.pagination.is_some() {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "stream responses do not support pagination",
            });
        }
        if plan.endpoint.response.no_content {
            return Err(ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "stream responses cannot use a no-content response plan",
            });
        }
        let dbg = plan
            .overrides
            .debug_level
            .unwrap_or_else(|| self.debug_level());
        let stream_response_limit = self.runtime_state.max_stream_response_body_bytes();
        let resp = match self
            .drive_attempts(
                &plan,
                &mut body,
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
        if !Self::header_matches_media_type(resp.headers().get(CONTENT_TYPE), M::CONTENT_TYPE) {
            return Err(ApiClientError::response_contract(
                ctx,
                "stream response content type did not match expected media type",
            ));
        }
        Ok(crate::stream_response::StreamResponse::new(resp))
    }
    async fn prepare_auth_plan(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        auth_state: &Cx::AuthState,
        executor: &dyn AuthHttpExecutor,
        head: &PublicRequestHead,
    ) -> Result<AuthPreparation, ApiClientError> {
        let mut summary = crate::auth::AuthAttemptSummary::default();
        let mut materials = Vec::new();
        let mut cacheable = !plan.endpoint.policy.auth.requirements.is_empty();
        for (requirement, slot) in plan
            .endpoint
            .policy
            .auth
            .requirements
            .iter()
            .zip(&head.auth_plan.slots)
        {
            let mut auth_request = crate::auth::AuthApplicationRequest::new(slot);
            let auth_meta = head.meta.clone();
            let prepared = crate::auth::prepare::<Cx>(
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
                    endpoint: head.meta.endpoint,
                    method: head.meta.method.clone(),
                },
                source,
            })?;
            prepared
                .validate_binding(slot)
                .map_err(|source| ApiClientError::Auth {
                    ctx: ErrorContext {
                        endpoint: head.meta.endpoint,
                        method: head.meta.method.clone(),
                    },
                    source,
                })?;
            if prepared.reuse != crate::auth::AuthPreparationReuse::RequestLocal {
                cacheable = false;
            }
            let applied = prepared.applied;
            summary.applied.push(applied);
            materials.push(prepared.material);
        }
        let cache_policy = if cacheable {
            AuthPreparationCachePolicy::RequestLocalReusable
        } else {
            AuthPreparationCachePolicy::Never
        };
        Ok(AuthPreparation {
            summary,
            materials,
            cache_policy,
        })
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
        if resp.meta().method == http::Method::HEAD && !no_content {
            return Err(ApiClientError::HeadRequiresNoContent { ctx });
        }
        if matches!(
            resp.status(),
            StatusCode::NO_CONTENT | StatusCode::RESET_CONTENT
        ) && !no_content
        {
            return Err(ApiClientError::NoContentStatusRequiresNoContent {
                ctx: ctx.clone(),
                status: resp.status(),
            });
        }
        let content_type = resp
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let status = resp.status();
        let (message, response_context) = resp.into_parts();
        let (parts, body) = message.into_parts();
        let value = C::decode(
            body,
            crate::codec::DecodeContext::new(
                ctx.endpoint,
                &ctx.method,
                status,
                content_type.as_deref(),
            ),
        )
        .map_err(|_| {
            ApiClientError::response_body_decode_error(ctx.clone(), status, content_type.as_deref())
        })?;
        Ok(DecodedResponse {
            meta: response_context.meta,
            url: response_context.request_url,
            status,
            headers: parts.headers,
            value,
        })
    }

    pub(super) fn debug_planned_request(
        &self,
        dbg: DebugLevel,
        built: &BuiltRequest,
        url_str: &str,
    ) {
        if dbg.is_verbose() {
            let request_context = built.context();
            self.debug_sink.request_start(
                dbg,
                &request_context.meta.method,
                url_str,
                request_context.meta.endpoint,
                request_context.meta.page_index,
            );
        }
        if dbg.is_very_verbose() {
            self.debug_sink.request_headers(
                dbg,
                crate::debug::SanitizedHeaders::new(built.message.headers()),
            );
        }
    }

    fn debug_planned_response(&self, dbg: DebugLevel, resp: &BuiltResponse, url_str: &str) {
        if dbg.is_verbose() {
            self.debug_sink
                .response_status(dbg, resp.status(), url_str, true);
        }
        if dbg.is_very_verbose() {
            self.debug_sink
                .response_headers(dbg, crate::debug::SanitizedHeaders::new(resp.headers()));
        }
    }

    #[cfg(feature = "dangerous-dev-tools")]
    #[allow(deprecated)]
    fn maybe_capture_dev_response_body(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        resp: &BuiltResponse,
    ) {
        let Some(capture) = self.runtime_state.dev_body_capture() else {
            return;
        };
        if !plan.endpoint.policy.auth.requirements.is_empty() {
            return;
        }
        capture.capture_response(
            plan.endpoint.meta.name,
            &plan.endpoint.meta.method,
            resp.status(),
            resp.body(),
        );
    }
}

fn into_canonical_request_plan_view(
    mut plan: RequestPlan,
) -> (crate::endpoint::RequestPlanView, crate::io::PreparedBody) {
    plan.endpoint.policy.rate_limit.canonicalize();
    let RequestPlan {
        endpoint,
        body,
        overrides,
    } = plan;
    (
        crate::endpoint::RequestPlanView {
            endpoint,
            overrides,
        },
        body,
    )
}

fn checked_attempt(
    base_attempt: u32,
    attempt_index: u32,
    ctx: &ErrorContext,
) -> Result<u32, ApiClientError> {
    base_attempt
        .checked_add(attempt_index)
        .ok_or_else(|| ApiClientError::PolicyViolation {
            ctx: ctx.clone(),
            msg: "request attempt counter overflowed",
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
        let err = checked_attempt(u32::MAX, 1, &ctx)
            .expect_err("overflowing base plus attempt should fail");
        assert!(
            err.to_string()
                .contains("request attempt counter overflowed")
        );
    }
}
