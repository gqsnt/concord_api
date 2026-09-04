//! One statically typed production engine for generated requests.

use std::future::IntoFuture;

use http::{Method, StatusCode};

use crate::auth::PreparedAuthPlan;
use crate::body::{
    Replayability, RequestBodyContext, RequestBodyMetadata, RequestRepresentation,
    validate_produced_body,
};
use crate::client::ClientCore;
use crate::endpoint::EndpointId;
use crate::error::{ApiError, ErrorCategory};
use crate::hooks::{
    AttemptInfo, AttemptStarted, BoxFuture, CompletionOutcome, DebugLevel, OperationEvent,
    OperationFinished, ResponseHeadObserved,
};
use crate::operation::{OperationDeadline, OperationId};
use crate::rate_limit::{RateLimitRequest, RateLimitResponse};
use crate::request::{
    AcceptedBytesRequest, EndpointSpec, OperationPreparation, PreparationFailure,
    PreparedOperation, PreparedPaginationPage, Request, ResponseRequest, prepare_operation,
};
use crate::response::{
    AcceptedBytesSelection, ApiResponse, DeclaredResponse, ExecutionMeta, ResponseHead,
    ResponseInput, ResponseRepresentation, ResponseSelection, completion_outcome, execute_response,
};
use crate::retry::{
    RetryAttempt, RetryDecision, RetryFailure, RetryPolicy, ServerDelayHint, parse_server_delay,
};

/// Hard aggregate visible-attempt ceiling shared by direct and paginated operations.
pub const MAX_VISIBLE_ATTEMPTS_PER_OPERATION: u32 = 4_096;

#[doc(hidden)]
impl<E> IntoFuture for Request<E>
where
    E: EndpointSpec,
{
    type Output = Result<E::Output, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { execute_request(self).await.map(ApiResponse::into_value) })
    }
}

#[doc(hidden)]
impl<E> IntoFuture for ResponseRequest<E>
where
    E: EndpointSpec,
{
    type Output = Result<ApiResponse<E::Output>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(execute_request(self.request))
    }
}

#[doc(hidden)]
impl<E> IntoFuture for AcceptedBytesRequest<E>
where
    E: EndpointSpec,
    <E::Response as ResponseRepresentation<E::Output>>::Driver:
        crate::response::AcceptedBytesDriver,
{
    type Output = Result<ApiResponse<bytes::Bytes>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            execute_request_as::<E, AcceptedBytesSelection<E::Response>>(self.request).await
        })
    }
}

async fn execute_request<E>(request: Request<E>) -> Result<ApiResponse<E::Output>, ApiError>
where
    E: EndpointSpec,
{
    execute_request_as::<E, DeclaredResponse<E::Response>>(request).await
}

async fn execute_request_as<E, S>(request: Request<E>) -> Result<ApiResponse<S::Output>, ApiError>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response, E::Output>,
{
    let prepared = match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => prepared,
        OperationPreparation::Failed(failure) => {
            return complete_preparation_failure(failure).await;
        }
    };
    execute_prepared_operation::<E, S>(
        prepared,
        ExecutionContext::DIRECT,
        MAX_VISIBLE_ATTEMPTS_PER_OPERATION,
    )
    .await
    .result
}

#[derive(Clone, Copy)]
struct ExecutionContext {
    parent_operation_id: Option<OperationId>,
    page_index: u32,
}

impl ExecutionContext {
    const DIRECT: Self = Self {
        parent_operation_id: None,
        page_index: 0,
    };

    const fn page(parent_operation_id: OperationId, page_index: u32) -> Self {
        Self {
            parent_operation_id: Some(parent_operation_id),
            page_index,
        }
    }
}

pub(crate) struct ExecutedPage<T> {
    pub(crate) operation_id: OperationId,
    pub(crate) result: Result<ApiResponse<T>, ApiError>,
    pub(crate) response_body_bytes: usize,
    pub(crate) final_status: Option<StatusCode>,
    pub(crate) attempts_started: u32,
    pub(crate) policy_retries: u32,
    pub(crate) recovered_authentication: bool,
}

pub(crate) async fn execute_pagination_page<E>(
    page: PreparedPaginationPage<E>,
    parent_operation_id: OperationId,
    page_index: u32,
    deadline: OperationDeadline,
    attempt_ceiling: u32,
) -> Result<ExecutedPage<E::Output>, ApiError>
where
    E: EndpointSpec,
{
    let PreparedPaginationPage {
        core,
        endpoint,
        head,
        body_metadata,
    } = page;
    let operation_id = core.allocate_operation_id(head.endpoint_id(), head.method())?;
    let context = ExecutionContext::page(parent_operation_id, page_index);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    Ok(
        execute_prepared_operation::<E, DeclaredResponse<E::Response>>(
            PreparedOperation {
                core,
                endpoint,
                operation_id,
                deadline,
                head,
                body_metadata,
            },
            context,
            attempt_ceiling,
        )
        .await,
    )
}

async fn execute_prepared_operation<E, S>(
    prepared: PreparedOperation<E>,
    context: ExecutionContext,
    attempt_ceiling: u32,
) -> ExecutedPage<S::Output>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response, E::Output>,
{
    let PreparedOperation {
        core,
        endpoint,
        operation_id,
        deadline,
        head,
        body_metadata,
    } = prepared;
    let mut final_status = None;
    let mut attempts_started = 1_u32;
    let mut policy_retries = 0_u32;
    let mut recovered_authentication = false;
    let mut authentication_recovery_used = false;
    let retry_policy = core.retry_policy().cloned();

    let result = async {
        let mut attempt_info = AttemptInfo::INITIAL;
        check_deadline(deadline, head.endpoint_id(), head.method())?;
        let input = endpoint.into_body();
        let body = <E::Body as RequestRepresentation<E::Input>>::prepare(
            input,
            RequestBodyContext {
                endpoint: head.endpoint(),
                method: head.method(),
                body_limit: core.request_body_limit(),
            },
        );
        check_deadline(deadline, head.endpoint_id(), head.method())?;
        let body = body.map_err(|error| request_body_error(&head, error))?;
        validate_produced_body(&body_metadata, &body)
            .map_err(|error| request_body_error(&head, error))?;
        check_deadline(deadline, head.endpoint_id(), head.method())?;
        let mut prepared_body = body
            .into_prepared(core.request_body_limit())
            .map_err(|error| request_body_error(&head, error))?;
        check_deadline(deadline, head.endpoint_id(), head.method())?;
        debug_assert_eq!(prepared_body.kind(), body_metadata.kind);
        let known_body_length = prepared_body.known_length();
        let materialized_metadata = body_metadata.clone().with_known_length(known_body_length);

        observe_attempt_started(
            &core,
            operation_id,
            context,
            deadline,
            &head,
            &materialized_metadata,
            attempt_info,
        )?;

        let mut prepared_auth = run_auth(
            &core,
            deadline,
            &head,
            head.auth().preflight(core.provider_http(Some(deadline))),
        )
        .await?;
        let response = loop {
            let attempt_metadata = materialized_metadata
                .clone()
                .with_known_length(prepared_body.known_length());
            let attempt = match send_attempt(
                &core,
                operation_id,
                deadline,
                &head,
                &mut prepared_body,
                &attempt_metadata,
                prepared_auth,
                attempt_info,
                &mut final_status,
                context,
            )
            .await
            {
                Ok(attempt) => attempt,
                Err(error) => {
                    let failure = match error.category() {
                        ErrorCategory::Connect => Some(RetryFailure::Connect),
                        ErrorCategory::Transport => Some(RetryFailure::Transport),
                        _ => None,
                    };
                    if let Some(failure) = failure {
                        let decision = classify_retry(
                            retry_policy.as_ref(),
                            deadline,
                            &head,
                            context,
                            attempt_info,
                            policy_retries,
                            failure,
                        )?;
                        if decision == RetryDecision::Retry
                            && admit_policy_retry(
                                PolicyRetryAdmission {
                                    policy: retry_policy.as_ref(),
                                    operation_id,
                                    context,
                                    deadline,
                                    head: &head,
                                    body_replayability: prepared_body.replayability(),
                                    attempt_ceiling,
                                    dispatch_knowledge: failure.dispatch_knowledge(),
                                    server_delay: None,
                                },
                                &mut attempt_info,
                                &mut attempts_started,
                                &mut policy_retries,
                            )
                            .await?
                        {
                            prepared_auth = prepare_policy_retry(
                                &core,
                                operation_id,
                                context,
                                deadline,
                                &head,
                                &body_metadata,
                                &mut prepared_body,
                                attempt_info,
                            )
                            .await?;
                            continue;
                        }
                    }
                    return Err(error);
                }
            };
            let status = attempt.response.status();

            if let Some(challenge_index) = attempt.applied_auth.challenge_index(status) {
                let reusable = prepared_body.replayability() != Replayability::OneShot;
                let can_recover = !authentication_recovery_used
                    && attempts_started < attempt_ceiling
                    && reusable
                    && attempt
                        .applied_auth
                        .challenge_can_recover(challenge_index)
                        .map_err(|error| auth_api_error(&head, error))?;
                if !can_recover {
                    attempt
                        .applied_auth
                        .apply_terminal_challenge(challenge_index)
                        .map_err(|error| auth_api_error(&head, error))?;
                    let failure = core
                        .application_http()
                        .terminal_status_managed(
                            head.endpoint_id().clone(),
                            head.method().clone(),
                            attempt.response,
                            attempt.server_delay.map(ServerDelayHint::delay),
                            deadline.expires_at(),
                        )
                        .await;
                    return Err(ApiError::authentication_rejected(failure));
                }

                // A response selected for recovery is dropped without polling its body.
                drop(attempt.response);
                authentication_recovery_used = true;
                let next_attempts_started = attempts_started
                    .checked_add(1)
                    .ok_or_else(|| internal_error(head.endpoint_id(), head.method()))?;
                attempt_info = AttemptInfo {
                    index: attempts_started,
                    policy_retry_index: policy_retries,
                    authentication_recovery: true,
                };
                attempts_started = next_attempts_started;
                if let Err(error) = observe_attempt_started(
                    &core,
                    operation_id,
                    context,
                    deadline,
                    &head,
                    &body_metadata,
                    attempt_info,
                ) {
                    let _ = attempt
                        .applied_auth
                        .apply_terminal_challenge(challenge_index);
                    return Err(error);
                }
                if let Err(error) =
                    prepared_body.reconstruct_for_next_attempt(core.request_body_limit())
                {
                    drop(error);
                    let primary = ApiError::classified(
                        head.endpoint_id().clone(),
                        head.method().clone(),
                        ErrorCategory::RequestBody,
                    );
                    let _ = attempt
                        .applied_auth
                        .apply_terminal_challenge(challenge_index);
                    return Err(primary);
                }
                if let Err(error) = check_deadline(deadline, head.endpoint_id(), head.method()) {
                    let _ = attempt
                        .applied_auth
                        .apply_terminal_challenge(challenge_index);
                    return Err(error);
                }
                if let Err(error) = run_auth(
                    &core,
                    deadline,
                    &head,
                    attempt
                        .applied_auth
                        .recover_attributed(challenge_index, core.provider_http(Some(deadline))),
                )
                .await
                {
                    let _ = attempt
                        .applied_auth
                        .apply_terminal_challenge(challenge_index);
                    return Err(error);
                }
                prepared_auth = run_auth(
                    &core,
                    deadline,
                    &head,
                    attempt
                        .applied_auth
                        .preflight_current(core.provider_http(Some(deadline))),
                )
                .await?;
                recovered_authentication = true;
                continue;
            }

            let accepted = head.accepted_status().contains(status);
            if accepted {
                break attempt.response;
            }

            let failure = RetryFailure::RejectedStatus {
                status,
                server_delay: attempt.server_delay.map(ServerDelayHint::delay),
            };
            let retry_decision = classify_retry(
                retry_policy.as_ref(),
                deadline,
                &head,
                context,
                attempt_info,
                policy_retries,
                failure,
            )?;
            if retry_decision == RetryDecision::Retry
                && admit_policy_retry(
                    PolicyRetryAdmission {
                        policy: retry_policy.as_ref(),
                        operation_id,
                        context,
                        deadline,
                        head: &head,
                        body_replayability: prepared_body.replayability(),
                        attempt_ceiling,
                        dispatch_knowledge: failure.dispatch_knowledge(),
                        server_delay: attempt.server_delay,
                    },
                    &mut attempt_info,
                    &mut attempts_started,
                    &mut policy_retries,
                )
                .await?
            {
                // A rejected response selected for retry is dropped without
                // polling its body.
                drop(attempt.response);
                prepared_auth = prepare_policy_retry(
                    &core,
                    operation_id,
                    context,
                    deadline,
                    &head,
                    &body_metadata,
                    &mut prepared_body,
                    attempt_info,
                )
                .await?;
                continue;
            }

            let failure = core
                .application_http()
                .terminal_status_managed(
                    head.endpoint_id().clone(),
                    head.method().clone(),
                    attempt.response,
                    attempt.server_delay.map(ServerDelayHint::delay),
                    deadline.expires_at(),
                )
                .await;
            return Err(ApiError::from_http_response(failure));
        };

        let status = response.status();
        let version = response.version();
        let headers = response.headers().clone();
        let content_length = response.content_length();
        let metadata = ExecutionMeta::new(
            head.endpoint_id().clone(),
            head.method().clone(),
            context.page_index,
            attempts_started,
            policy_retries,
            recovered_authentication,
        );
        let response_head = ResponseHead {
            endpoint: head.endpoint(),
            method: head.method(),
            url: head.logical_url(),
            status,
            version,
            headers: &headers,
            content_length,
            metadata: &metadata,
        };
        let (output, response_body_bytes) = execute_response::<E::Response, E::Output, S>(
            ResponseInput {
                response,
                limit: core.response_body_limit(),
                stream_limit: core.stream_response_limit(),
                deadline,
                endpoint: head.endpoint_id().clone(),
                method: head.method().clone(),
                stream_observation: Some(crate::response::StreamObservation::new(
                    core.operation_observer().clone(),
                    operation_id,
                    context.parent_operation_id,
                    head.endpoint_id().clone(),
                    head.method().clone(),
                )),
            },
            &response_head,
        )
        .await?;
        Ok((
            ApiResponse::new(
                head.logical_url().clone(),
                status,
                version,
                headers,
                metadata,
                output,
            ),
            response_body_bytes,
        ))
    }
    .await;

    let fixed_at = OperationDeadline::now();
    let elapsed = deadline.elapsed_at(fixed_at);
    let result = deliver_completion(
        &core,
        operation_id,
        context,
        head.endpoint(),
        head.method(),
        head.request_target(),
        head.debug(),
        elapsed,
        final_status,
        completion_outcome::<E::Response, E::Output, S>(),
        attempts_started,
        policy_retries,
        recovered_authentication,
        result,
    )
    .await;
    let (result, response_body_bytes) = match result {
        Ok((response, response_body_bytes)) => (Ok(response), response_body_bytes),
        Err(error) => (Err(error), 0),
    };
    ExecutedPage {
        operation_id,
        result,
        response_body_bytes,
        final_status,
        attempts_started,
        policy_retries,
        recovered_authentication,
    }
}

fn classify_retry(
    policy: Option<&RetryPolicy>,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    context: ExecutionContext,
    attempt: AttemptInfo,
    policy_retries: u32,
    failure: RetryFailure,
) -> Result<RetryDecision, ApiError> {
    let Some(policy) = policy else {
        return Ok(RetryDecision::Stop);
    };
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let decision = policy.classify(RetryAttempt::new(
        head.endpoint(),
        head.method(),
        head.logical_url().host_str(),
        context.page_index,
        attempt,
        policy_retries,
        failure,
    ));
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    Ok(decision)
}

struct PolicyRetryAdmission<'a> {
    policy: Option<&'a RetryPolicy>,
    operation_id: OperationId,
    context: ExecutionContext,
    deadline: OperationDeadline,
    head: &'a crate::request::PreparedHead,
    body_replayability: Replayability,
    attempt_ceiling: u32,
    dispatch_knowledge: crate::error::DispatchKnowledge,
    server_delay: Option<ServerDelayHint>,
}

async fn admit_policy_retry(
    admission: PolicyRetryAdmission<'_>,
    attempt: &mut AttemptInfo,
    attempts_started: &mut u32,
    policy_retries: &mut u32,
) -> Result<bool, ApiError> {
    let PolicyRetryAdmission {
        policy,
        operation_id,
        context,
        deadline,
        head,
        body_replayability,
        attempt_ceiling,
        dispatch_knowledge,
        server_delay,
    } = admission;
    let Some(policy) = policy else {
        return Ok(false);
    };

    // Retry authority is intentionally ordered: semantic resend permission,
    // the per-operation maximum, body replayability, then shared budget
    // admission.
    if head.effect() == crate::endpoint::Effect::NonIdempotent
        && dispatch_knowledge != crate::error::DispatchKnowledge::NotDispatched
    {
        return Ok(false);
    }
    if *policy_retries >= policy.max_retries_per_operation() {
        return Ok(false);
    }
    if body_replayability == Replayability::OneShot {
        return Ok(false);
    }

    let next_policy_retries = policy_retries
        .checked_add(1)
        .ok_or_else(|| internal_error(head.endpoint_id(), head.method()))?;
    let next_attempts_started = attempts_started
        .checked_add(1)
        .ok_or_else(|| internal_error(head.endpoint_id(), head.method()))?;
    if next_attempts_started > attempt_ceiling {
        return Ok(false);
    }

    let entropy = operation_id.retry_entropy(next_policy_retries, context.page_index);
    let Some(delay) = policy.retry_delay(
        next_policy_retries,
        entropy,
        server_delay.map(ServerDelayHint::delay),
    ) else {
        return Ok(false);
    };
    if deadline
        .remaining()
        .is_some_and(|remaining| delay >= remaining)
    {
        return Ok(false);
    }
    if !delay.is_zero() {
        deadline
            .run(tokio::time::sleep(delay))
            .await
            .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?;
    }
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    if !policy.reserve_retry(head.logical_url()) {
        return Ok(false);
    }

    *attempt = AttemptInfo {
        index: *attempts_started,
        policy_retry_index: next_policy_retries,
        authentication_recovery: false,
    };
    *attempts_started = next_attempts_started;
    *policy_retries = next_policy_retries;
    Ok(true)
}

#[allow(
    clippy::too_many_arguments,
    reason = "retry preparation receives the exact bounded operation state"
)]
async fn prepare_policy_retry(
    core: &ClientCore,
    operation_id: OperationId,
    context: ExecutionContext,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    body_metadata: &RequestBodyMetadata,
    prepared_body: &mut crate::request::PreparedBody,
    attempt: AttemptInfo,
) -> Result<PreparedAuthPlan, ApiError> {
    observe_attempt_started(
        core,
        operation_id,
        context,
        deadline,
        head,
        body_metadata,
        attempt,
    )?;
    prepared_body
        .reconstruct_for_next_attempt(core.request_body_limit())
        .map_err(|error| {
            drop(error);
            ApiError::classified(
                head.endpoint_id().clone(),
                head.method().clone(),
                ErrorCategory::RequestBody,
            )
        })?;
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let prepared_auth = run_auth(
        core,
        deadline,
        head,
        head.auth().preflight(core.provider_http(Some(deadline))),
    )
    .await?;
    Ok(prepared_auth)
}

async fn complete_preparation_failure<E>(
    failure: PreparationFailure,
) -> Result<ApiResponse<E>, ApiError> {
    let PreparationFailure {
        core,
        operation_id,
        meta,
        target,
        debug,
        elapsed,
        error,
    } = failure;
    deliver_completion(
        &core,
        operation_id,
        ExecutionContext::DIRECT,
        meta.endpoint(),
        meta.method(),
        target,
        debug,
        elapsed,
        None,
        CompletionOutcome::Success,
        0,
        0,
        false,
        Err(error),
    )
    .await
}

fn observe_attempt_started(
    core: &ClientCore,
    operation_id: OperationId,
    context: ExecutionContext,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    body_metadata: &RequestBodyMetadata,
    attempt: AttemptInfo,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let event = attempt_started_event(operation_id, context, head, body_metadata, attempt);
    crate::hooks::observe(
        core.operation_observer().as_ref(),
        OperationEvent::AttemptStarted(event),
    );
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    Ok(())
}

fn observe_response_head(
    core: &ClientCore,
    deadline: OperationDeadline,
    endpoint_id: &EndpointId,
    event: ResponseHeadObserved<'_>,
) -> Result<(), ApiError> {
    check_deadline(deadline, endpoint_id, event.method)?;
    let method = event.method.clone();
    crate::hooks::observe(
        core.operation_observer().as_ref(),
        OperationEvent::ResponseHeadObserved(event),
    );
    check_deadline(deadline, endpoint_id, &method)
}

fn attempt_started_event<'a>(
    operation_id: OperationId,
    context: ExecutionContext,
    head: &'a crate::request::PreparedHead,
    body_metadata: &RequestBodyMetadata,
    attempt: AttemptInfo,
) -> AttemptStarted<'a> {
    AttemptStarted {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        target: head.request_target(),
        page_index: context.page_index,
        attempt,
        body_kind: body_metadata.kind,
        known_body_length: body_metadata.known_length(),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the completion event has exact normative fields"
)]
async fn deliver_completion<T>(
    core: &ClientCore,
    operation_id: OperationId,
    context: ExecutionContext,
    endpoint: &str,
    method: &Method,
    target: crate::hooks::RequestTarget,
    debug: DebugLevel,
    elapsed: std::time::Duration,
    final_status: Option<StatusCode>,
    success_outcome: CompletionOutcome,
    attempts_started: u32,
    policy_retries: u32,
    recovered_authentication: bool,
    result: Result<T, ApiError>,
) -> Result<T, ApiError> {
    let attempt = attempts_started.checked_sub(1).map(|index| AttemptInfo {
        index,
        policy_retry_index: policy_retries,
        authentication_recovery: recovered_authentication && index > policy_retries,
    });
    let result = result
        .map_err(|error| error.with_execution_context(operation_id, attempt, context.page_index));
    let outcome = match &result {
        Ok(_) => success_outcome,
        Err(error) => CompletionOutcome::from_error_category(error.category()),
    };
    let event = OperationFinished {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint,
        method,
        target,
        page_index: context.page_index,
        attempts_started,
        policy_retries,
        recovered_authentication,
        elapsed,
        final_status,
        outcome,
    };
    core.debug_dispatcher(debug).completion(&event);
    crate::hooks::observe(
        core.operation_observer().as_ref(),
        OperationEvent::OperationFinished(event),
    );
    result
}

struct AttemptResponse {
    response: crate::http::application::ApplicationResponse,
    applied_auth: PreparedAuthPlan,
    server_delay: Option<ServerDelayHint>,
}

#[allow(
    clippy::too_many_arguments,
    reason = "one attempt receives the exact canonical lifecycle state"
)]
async fn send_attempt(
    core: &ClientCore,
    operation_id: OperationId,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    prepared_body: &mut crate::request::PreparedBody,
    body_metadata: &RequestBodyMetadata,
    prepared_auth: PreparedAuthPlan,
    attempt: AttemptInfo,
    final_status: &mut Option<StatusCode>,
    context: ExecutionContext,
) -> Result<AttemptResponse, ApiError> {
    let native_body = prepared_body
        .prepare_native(core.request_body_limit())
        .map_err(|error| {
            drop(error);
            ApiError::classified(
                head.endpoint_id().clone(),
                head.method().clone(),
                ErrorCategory::RequestBody,
            )
        })?;
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    acquire_rate_limit_authority(core, context, attempt, deadline, head).await?;
    let (applied_auth, finalized_auth) = run_auth(
        core,
        deadline,
        head,
        prepared_auth.finalize(core.provider_http(Some(deadline))),
    )
    .await?;
    let mut request = core.application_http().build_request(
        head.endpoint_id(),
        head.method(),
        head.logical_url(),
        head.public_headers(),
        native_body,
    )?;
    let request_event = attempt_started_event(operation_id, context, head, body_metadata, attempt);
    core.debug_dispatcher(head.debug())
        .request_ready(&request_event);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    finalized_auth.materialize(&mut request, head.endpoint_id(), head.method())?;
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    if attempt.index == 0 {
        if let Some(policy) = core.retry_policy() {
            policy.record_initial_dispatch(head.logical_url());
        }
    }

    let response = deadline
        .run(core.application_http().execute(
            head.endpoint_id(),
            head.method(),
            request,
            head.observed_url(),
            head.public_headers(),
            head.pagination_sensitive_headers(),
            body_metadata.kind,
            body_metadata.known_length(),
        ))
        .await
        .map_err(|_| dispatched_timeout_error(head.endpoint_id(), head.method()))??;

    let status = response.status();
    let response_wall_time = core.response_wall_time();
    let server_delay = parse_server_delay(response.headers(), response_wall_time);
    *final_status = Some(status);
    let response_event = ResponseHeadObserved {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        target: head.request_target(),
        page_index: context.page_index,
        attempt,
        status,
        content_length: parsed_content_length(response.headers()),
        server_delay: server_delay.map(ServerDelayHint::delay),
    };
    core.debug_dispatcher(head.debug())
        .response_head(&response_event);
    observe_rate_limit_response(core, context, attempt, head, status, server_delay);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    observe_response_head(core, deadline, head.endpoint_id(), response_event)?;
    Ok(AttemptResponse {
        response,
        applied_auth,
        server_delay,
    })
}

async fn run_auth<T>(
    _core: &ClientCore,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    future: impl std::future::Future<Output = Result<T, crate::auth::AuthError>>,
) -> Result<T, ApiError> {
    deadline
        .run(future)
        .await
        .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?
        .map_err(|error| auth_api_error(head, error))
}

fn auth_api_error(head: &crate::request::PreparedHead, error: crate::auth::AuthError) -> ApiError {
    ApiError::authentication(head.endpoint_id().clone(), head.method().clone(), error)
}

fn request_body_error(
    head: &crate::request::PreparedHead,
    error: crate::body::BodyError,
) -> ApiError {
    let category = if error.kind() == crate::body::BodyErrorKind::Producer {
        ErrorCategory::RequestEncoding
    } else {
        ErrorCategory::RequestBody
    };
    drop(error);
    ApiError::classified(head.endpoint_id().clone(), head.method().clone(), category)
}

async fn acquire_rate_limit_authority(
    core: &ClientCore,
    context: ExecutionContext,
    attempt: AttemptInfo,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let request = RateLimitRequest {
        endpoint: head.endpoint(),
        method: head.method(),
        logical_origin: head.rate_limit_logical_origin(),
        page_index: context.page_index,
        attempt,
        plan: head.rate_limit_plan(),
    };
    let future = core.rate_limiter().acquire(request);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    deadline
        .run(future)
        .await
        .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?
        .map(|_permit| ())
        .map_err(|error| {
            ApiError::rate_limit(head.endpoint_id().clone(), head.method().clone(), error)
        })
}

fn observe_rate_limit_response(
    core: &ClientCore,
    context: ExecutionContext,
    attempt: AttemptInfo,
    head: &crate::request::PreparedHead,
    status: StatusCode,
    server_delay: Option<ServerDelayHint>,
) {
    let response = RateLimitResponse {
        request: RateLimitRequest {
            endpoint: head.endpoint(),
            method: head.method(),
            logical_origin: head.rate_limit_logical_origin(),
            page_index: context.page_index,
            attempt,
            plan: head.rate_limit_plan(),
        },
        status,
        server_delay: server_delay.map(ServerDelayHint::delay),
        cooldown_cap: core.rate_limit_cooldown_cap(),
    };
    core.rate_limiter().observe(response);
}

fn parsed_content_length(headers: &http::HeaderMap) -> Option<u64> {
    let value = headers.get(http::header::CONTENT_LENGTH)?.to_str().ok()?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.bytes().try_fold(0_u64, |length, byte| {
        length.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
    })
}

fn check_deadline(
    deadline: OperationDeadline,
    endpoint: &EndpointId,
    method: &Method,
) -> Result<(), ApiError> {
    deadline
        .check()
        .map_err(|_| timeout_error(endpoint, method))
}

fn timeout_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout)
}

fn dispatched_timeout_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout).with_stage(
        crate::error::FailureStage::DuringDispatchOrUnknown,
        crate::error::DispatchKnowledge::PossiblyDispatched,
    )
}

fn internal_error(endpoint: &EndpointId, method: &Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Internal)
}

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod tests;
