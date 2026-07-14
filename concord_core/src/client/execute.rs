// Client lifecycle phase modules intentionally share one private parent namespace.
use super::build::PublicRequestHead;
use super::send_flow::ObservedExecutionResponse;
use super::*;

enum AuthRejectionStep {
    Retry,
    Fail(ApiClientError),
}

#[derive(Clone)]
struct AuthResendIntent {
    rejection_plan: crate::auth::AuthRejectionPlan,
    status: StatusCode,
    response_meta: RequestExecutionMeta,
    auth_attempt: crate::auth::AuthAttemptSummary,
    error_ctx: ErrorContext,
    #[cfg(any(test, feature = "dangerous-dev-tools"))]
    lifecycle_observation_targets: Vec<AuthLifecycleObservationTarget>,
}

#[derive(Clone, Copy)]
struct ChallengeTerminalStatusCtx<'a> {
    dbg: DebugLevel,
    dbg_verbose: bool,
    url_str: &'a str,
    error_ctx: &'a ErrorContext,
}

enum RecoverableChallengeStep {
    Recover(AuthResendIntent),
    InvalidateAndFail(AuthResendIntent),
}

#[derive(Clone, Copy)]
enum AuthRejectionApplication {
    ProviderCapable,
    InvalidationOnly,
}

#[derive(Debug, Default)]
struct AuthenticationRecoveryBudget {
    initiated: bool,
}

impl AuthenticationRecoveryBudget {
    fn initiate(&mut self) -> bool {
        if self.initiated {
            return false;
        }
        self.initiated = true;
        true
    }
}

struct AuthRejectionStepCtx<'a, Cx: ClientContext> {
    plan: &'a crate::endpoint::RequestPlanView,
    auth_state: &'a Cx::AuthState,
    auth_http: &'a ClientAuthHttpExecutor<'a, Cx>,
    response_meta: &'a RequestExecutionMeta,
    auth_attempt: &'a crate::auth::AuthAttemptSummary,
    rejection_plan: &'a crate::auth::AuthRejectionPlan,
    status: StatusCode,
    error_ctx: &'a ErrorContext,
    auth_rebuildable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutionFamily {
    Buffered {
        skip_body: bool,
        response_limit: Option<usize>,
    },
    Stream {
        response_limit: Option<usize>,
    },
}

enum ExecutionTransportSuccess {
    Buffered(ExecutionResponse),
    Transport(ExecutionResponse),
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

impl<Cx: ClientContext> ApiClient<Cx> {
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

    fn auth_plan_mismatch(meta: &RequestExecutionMeta, message: &'static str) -> ApiClientError {
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
        ctx: AuthRejectionStepCtx<'_, Cx>,
    ) -> Result<AuthRejectionStep, ApiClientError> {
        if !ctx.auth_rebuildable && ctx.rejection_plan.requests_recovery() {
            return Ok(AuthRejectionStep::Fail(Self::auth_challenge_rejected(
                ctx.error_ctx,
            )));
        }
        self.apply_auth_rejection_plan(&ctx, AuthRejectionApplication::ProviderCapable)
            .await?;
        if ctx.rejection_plan.requests_recovery() {
            Ok(AuthRejectionStep::Retry)
        } else {
            Ok(AuthRejectionStep::Fail(Self::auth_challenge_rejected(
                ctx.error_ctx,
            )))
        }
    }

    async fn apply_auth_rejection_plan(
        &self,
        ctx: &AuthRejectionStepCtx<'_, Cx>,
        application: AuthRejectionApplication,
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
                        "authentication rejection plan does not match the applied credential",
                    ),
                });
            };
            bindings.push(binding);
        }

        for (action_index, requirement_index, applied_index) in bindings {
            let action = &ctx.rejection_plan.actions()[action_index];
            let requirement = &requirements[requirement_index];
            let applied = &applied[applied_index];
            let result = match application {
                AuthRejectionApplication::ProviderCapable => {
                    crate::auth::apply_rejection::<Cx>(
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
                    .await
                }
                AuthRejectionApplication::InvalidationOnly => {
                    crate::auth::apply_rejection_invalidation_only::<Cx>(
                        action,
                        requirement,
                        applied,
                        self.vars(),
                        self.auth_vars(),
                        ctx.auth_state,
                        ctx.response_meta,
                        ctx.status,
                    )
                    .await
                }
            };
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

    fn recoverable_challenge_step(
        budget: &mut AuthenticationRecoveryBudget,
        intent: AuthResendIntent,
    ) -> RecoverableChallengeStep {
        if budget.initiate() {
            RecoverableChallengeStep::Recover(intent)
        } else {
            RecoverableChallengeStep::InvalidateAndFail(intent)
        }
    }

    fn release_challenged_response(
        &self,
        observed: ObservedExecutionResponse,
        intent: &AuthResendIntent,
        terminal_status: Option<ChallengeTerminalStatusCtx<'_>>,
    ) -> Option<ApiClientError> {
        #[cfg(all(not(test), not(feature = "dangerous-dev-tools")))]
        let _ = intent;
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        let matched_targets = intent
            .lifecycle_observation_targets
            .iter()
            .filter(|target| {
                intent
                    .rejection_plan
                    .actions()
                    .iter()
                    .any(|action| target.matches(action))
            })
            .collect::<Vec<_>>();

        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        for target in &matched_targets {
            target
                .target
                .emit(crate::auth::CredentialLifecycleEvent::ChallengeClassified {
                    status: intent.status,
                });
        }

        let ObservedExecutionResponse {
            response,
            rate_limit_action,
        } = observed;
        let terminal = terminal_status.map(|terminal| {
            if terminal.dbg_verbose {
                self.debug_sink.response_status(
                    terminal.dbg,
                    response.status(),
                    terminal.url_str,
                    false,
                );
                self.debug_sink.response_headers(
                    terminal.dbg,
                    crate::debug::SanitizedHeaders::new(response.headers()),
                );
            }
            ApiClientError::HttpStatus {
                ctx: terminal.error_ctx.clone(),
                status: response.status(),
                headers: Box::new(crate::redaction::sanitize_header_map(response.headers())),
                rate_limit: (!matches!(rate_limit_action, RateLimitResponseAction::Continue))
                    .then_some(Box::new(rate_limit_action)),
            }
        });

        // Dropping the native response here is the single challenged-response
        // transition. No body frame is polled before credential mutation.
        drop(response);

        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        for target in matched_targets {
            target
                .target
                .emit(crate::auth::CredentialLifecycleEvent::ResponseReleased);
        }

        terminal
    }

    async fn invalidate_exhausted_auth_challenge(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        auth_state: &Cx::AuthState,
        auth_http: &ClientAuthHttpExecutor<'_, Cx>,
        intent: &AuthResendIntent,
        auth_rebuildable: bool,
    ) -> Result<ApiClientError, ApiClientError> {
        self.invalidate_auth_challenge_only(plan, auth_state, auth_http, intent, auth_rebuildable)
            .await?;
        Ok(Self::auth_challenge_rejected(&intent.error_ctx))
    }

    async fn invalidate_auth_challenge_only(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        auth_state: &Cx::AuthState,
        auth_http: &ClientAuthHttpExecutor<'_, Cx>,
        intent: &AuthResendIntent,
        auth_rebuildable: bool,
    ) -> Result<(), ApiClientError> {
        self.apply_auth_rejection_plan(
            &AuthRejectionStepCtx {
                plan,
                auth_state,
                auth_http,
                response_meta: &intent.response_meta,
                auth_attempt: &intent.auth_attempt,
                rejection_plan: &intent.rejection_plan,
                status: intent.status,
                error_ctx: &intent.error_ctx,
                auth_rebuildable,
            },
            AuthRejectionApplication::InvalidationOnly,
        )
        .await
    }

    fn auth_challenge_rejected(ctx: &ErrorContext) -> ApiClientError {
        ApiClientError::Auth {
            ctx: ctx.clone(),
            source: AuthError::new(AuthErrorKind::ProviderRejected, "auth challenge rejected"),
        }
    }

    async fn drive_executions(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        body: &mut crate::io::PreparedBody,
        ctx: ErrorContext,
        dbg: DebugLevel,
        family: ExecutionFamily,
    ) -> Result<ExecutionTransportSuccess, ApiClientError> {
        let dbg_verbose = dbg.is_verbose();
        // The authoritative logical body recipe determines reconstruction
        // capacity for the single bounded authentication recovery only. Reqwest
        // owns hidden request resends. A non-rebuildable body is not rejected pre-execution merely
        // because a challenge could occur; instead a challenge simply cannot
        // trigger a second execution.
        let auth_rebuildable = body.is_replayable();
        let mut auth_state_snapshot = None;
        let auth_http = ClientAuthHttpExecutor { client: self };
        let mut auth_placement_plan: Option<crate::auth::AuthPlacementPlan> = None;
        // At most one bounded authentication recovery. When set, the next
        // iteration applies the credential refresh and performs a second
        // visible execution.
        let mut pending_auth: Option<AuthResendIntent> = None;
        let mut auth_recovery = AuthenticationRecoveryBudget::default();
        // Request-local auth preparation cache, reused for the recovery unless
        // the challenge handling asked for a refreshed credential state.
        let mut cached_auth_preparation: Option<CachedAuthPreparation> = None;

        loop {
            let meta = plan.endpoint.meta.request_meta(plan.overrides.page_index);
            let mut head = self.resolve_public_request_head(plan, body, meta)?;
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
            self.managed_client
                .preflight_url(&head.url)
                .map_err(|_| ApiClientError::TlsCapabilityUnavailable { ctx: ctx.clone() })?;
            if auth_state_snapshot.is_none() {
                auth_state_snapshot =
                    Some(
                        self.try_auth_state()
                            .map_err(|source| ApiClientError::Auth {
                                ctx: ctx.clone(),
                                source,
                            })?,
                    );
            }
            let auth_state_snapshot = auth_state_snapshot
                .as_ref()
                .expect("TLS preflight initializes the authentication state snapshot");
            if let Some(intent) = pending_auth.take() {
                // pending_auth is only ever set for a rebuildable body, so the
                // credential refresh below always has a reconstructable request.
                let step = self
                    .apply_auth_rejection_step(AuthRejectionStepCtx {
                        plan,
                        auth_state: auth_state_snapshot,
                        auth_http: &auth_http,
                        response_meta: &intent.response_meta,
                        auth_attempt: &intent.auth_attempt,
                        rejection_plan: &intent.rejection_plan,
                        status: intent.status,
                        error_ctx: &intent.error_ctx,
                        auth_rebuildable,
                    })
                    .await?;
                match step {
                    AuthRejectionStep::Retry => {
                        cached_auth_preparation = None;
                    }
                    AuthRejectionStep::Fail(err) => return Err(err),
                }
            }
            let auth_preparation = if cached_auth_preparation.is_none() {
                Some(
                    self.prepare_auth(plan, auth_state_snapshot, &auth_http, &head)
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
            let execution_body = self.produce_execution_body(body, &ctx)?;
            let built = head.finish(&self.managed_client.client, execution_body, &ctx)?;
            let url_str = built.debug_url();
            let send_ctx = SendClassifyCtx {
                dbg,
                url_str: &url_str,
                error_ctx: &ctx,
                auth_materials: &auth_attempt.materials,
            };
            let send_result = self.send_and_observe_once(built, send_ctx).await;

            match send_result {
                Ok(observed) => {
                    let response_status = observed.response.status();
                    let response_meta = &observed.response.context.meta;
                    let response_headers = observed.response.headers();
                    let classification = self.classify_auth_rejection(AuthRejectionCtx {
                        plan,
                        auth_state: auth_state_snapshot,
                        meta: response_meta,
                        status: response_status,
                        headers: response_headers,
                        auth_attempt: &auth_attempt.summary,
                    })?;
                    match classification {
                        Some(rejection_plan) if rejection_plan.requests_recovery() => {
                            let intent = AuthResendIntent {
                                rejection_plan,
                                status: response_status,
                                response_meta: response_meta.clone(),
                                auth_attempt: auth_attempt.summary.clone(),
                                error_ctx: ctx.clone(),
                                #[cfg(any(test, feature = "dangerous-dev-tools"))]
                                lifecycle_observation_targets: auth_attempt
                                    .lifecycle_observation_targets
                                    .clone(),
                            };
                            if !auth_rebuildable {
                                let terminal = self
                                    .release_challenged_response(
                                        observed,
                                        &intent,
                                        Some(ChallengeTerminalStatusCtx {
                                            dbg,
                                            dbg_verbose,
                                            url_str: &url_str,
                                            error_ctx: &ctx,
                                        }),
                                    )
                                    .expect("terminal challenge status must be captured");
                                // Rebuildability controls only whether another
                                // visible execution is possible. The rejected
                                // applied generation must still be invalidated
                                // before returning its original status path.
                                self.invalidate_auth_challenge_only(
                                    plan,
                                    auth_state_snapshot,
                                    &auth_http,
                                    &intent,
                                    auth_rebuildable,
                                )
                                .await?;
                                return Err(terminal);
                            } else {
                                match Self::recoverable_challenge_step(&mut auth_recovery, intent) {
                                    RecoverableChallengeStep::Recover(intent) => {
                                        self.release_challenged_response(observed, &intent, None);
                                        pending_auth = Some(intent);
                                        continue;
                                    }
                                    RecoverableChallengeStep::InvalidateAndFail(intent) => {
                                        self.release_challenged_response(observed, &intent, None);
                                        let terminal = self
                                            .invalidate_exhausted_auth_challenge(
                                                plan,
                                                auth_state_snapshot,
                                                &auth_http,
                                                &intent,
                                                auth_rebuildable,
                                            )
                                            .await?;
                                        return Err(terminal);
                                    }
                                }
                            }
                        }
                        Some(rejection_plan) => {
                            let intent = AuthResendIntent {
                                rejection_plan,
                                status: response_status,
                                response_meta: response_meta.clone(),
                                auth_attempt: auth_attempt.summary.clone(),
                                error_ctx: ctx.clone(),
                                #[cfg(any(test, feature = "dangerous-dev-tools"))]
                                lifecycle_observation_targets: auth_attempt
                                    .lifecycle_observation_targets
                                    .clone(),
                            };
                            self.release_challenged_response(observed, &intent, None);
                            match self
                                .apply_auth_rejection_step(AuthRejectionStepCtx {
                                    plan,
                                    auth_state: auth_state_snapshot,
                                    auth_http: &auth_http,
                                    response_meta: &intent.response_meta,
                                    auth_attempt: &auth_attempt.summary,
                                    rejection_plan: &intent.rejection_plan,
                                    status: response_status,
                                    error_ctx: &ctx,
                                    auth_rebuildable,
                                })
                                .await?
                            {
                                AuthRejectionStep::Fail(err) => return Err(err),
                                AuthRejectionStep::Retry => {
                                    return Err(Self::auth_challenge_rejected(&ctx));
                                }
                            }
                        }
                        None => {}
                    }
                    // Both buffered and streaming families perform terminal
                    // status classification only after authentication has
                    // inspected the unconsumed response head. Hooks and
                    // rate-limit feedback were already run exactly once by
                    // `send_and_observe_once`.
                    let emit_success_debug = matches!(family, ExecutionFamily::Stream { .. });
                    let resp = self.classify_observed_transport_response(
                        observed,
                        dbg,
                        dbg_verbose,
                        &url_str,
                        &ctx,
                        emit_success_debug,
                    )?;
                    let resp = match family {
                        ExecutionFamily::Buffered {
                            skip_body,
                            response_limit,
                        } => {
                            let limit = (!skip_body).then_some(response_limit).flatten();
                            ExecutionTransportSuccess::Buffered(Self::limit_response_body(
                                resp, limit, &ctx,
                            )?)
                        }
                        ExecutionFamily::Stream { response_limit } => {
                            ExecutionTransportSuccess::Transport(Self::limit_response_body(
                                resp,
                                response_limit,
                                &ctx,
                            )?)
                        }
                    };
                    return Ok(resp);
                }
                Err(err) => {
                    // No general retry: Reqwest owns any hidden resend, so a
                    // transport or observation failure is terminal here.
                    return Err(err);
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
            .drive_executions(
                &plan,
                &mut body,
                ctx.clone(),
                dbg,
                ExecutionFamily::Buffered {
                    skip_body: plan.endpoint.response.no_content,
                    response_limit: self.runtime_state.max_response_body_bytes(),
                },
            )
            .await?
        {
            ExecutionTransportSuccess::Buffered(resp) => resp,
            _ => unreachable!(),
        };
        let resp = Self::buffer_response(resp, plan.endpoint.response.no_content, &ctx).await?;
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
            .drive_executions(
                &plan,
                &mut body,
                ctx.clone(),
                dbg,
                ExecutionFamily::Buffered {
                    skip_body,
                    response_limit: self.runtime_state.max_response_body_bytes(),
                },
            )
            .await?
        {
            ExecutionTransportSuccess::Buffered(resp) => resp,
            _ => unreachable!(),
        };
        let resp = Self::buffer_response(resp, skip_body, &ctx).await?;
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
            .drive_executions(
                &plan,
                &mut body,
                ctx.clone(),
                dbg,
                ExecutionFamily::Stream {
                    response_limit: stream_response_limit,
                },
            )
            .await?
        {
            ExecutionTransportSuccess::Transport(resp) => resp,
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
        #[cfg(any(test, feature = "dangerous-dev-tools"))]
        let mut lifecycle_observation_targets = Vec::new();
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
            #[cfg(any(test, feature = "dangerous-dev-tools"))]
            if let Some(target) = prepared.lifecycle_observation_target {
                lifecycle_observation_targets.push(AuthLifecycleObservationTarget {
                    credential_id: prepared.applied.credential_id.clone(),
                    usage_id: prepared.applied.usage_id.clone(),
                    step_id: prepared.applied.step_id,
                    target,
                });
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
            #[cfg(any(test, feature = "dangerous-dev-tools"))]
            lifecycle_observation_targets,
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
            url: response_context.logical_url,
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
