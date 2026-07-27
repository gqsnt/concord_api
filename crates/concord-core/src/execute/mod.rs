//! One statically typed production engine for generated requests.

use std::future::IntoFuture;

use http::{Method, StatusCode};

use crate::auth::PreparedAuthPlan;
use crate::body::{
    Replayability, RequestBodyContext, RequestBodyMetadata, RequestBodySpec, validate_produced_body,
};
use crate::client::ClientCore;
use crate::endpoint::EndpointId;
use crate::error::{ApiError, ErrorCategory};
use crate::hooks::{
    AttemptInfo, BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel,
    ResponseHeadEvent,
};
use crate::operation::{OperationDeadline, OperationId};
use crate::rate_limit::{RateLimitRequest, RateLimitResponse};
use crate::redaction::SanitizedHeaders;
use crate::request::{
    EndpointSpec, OperationPreparation, PreparationFailure, PreparedOperation,
    PreparedPaginationPage, Request, ResponseBytesRequest, ResponseRequest, prepare_operation,
};
use crate::response::{
    ApiResponse, DeclaredResponse, ExecutionMeta, ResponseBodySpec, ResponseBytesSelection,
    ResponseHead, ResponseInput, ResponseSelection, completion_outcome, execute_response,
};

#[doc(hidden)]
impl<E> IntoFuture for Request<E>
where
    E: EndpointSpec,
{
    type Output = Result<<E::Response as ResponseBodySpec>::Output, ApiError>;
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
    type Output = Result<ApiResponse<<E::Response as ResponseBodySpec>::Output>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(execute_request(self.request))
    }
}

#[doc(hidden)]
impl<E> IntoFuture for ResponseBytesRequest<E>
where
    E: EndpointSpec,
    <E::Response as ResponseBodySpec>::Mode: crate::response::ResponseBytesMode,
{
    type Output = Result<ApiResponse<bytes::Bytes>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            execute_request_as::<E, ResponseBytesSelection<E::Response>>(self.request).await
        })
    }
}

async fn execute_request<E>(
    request: Request<E>,
) -> Result<ApiResponse<<E::Response as ResponseBodySpec>::Output>, ApiError>
where
    E: EndpointSpec,
{
    execute_request_as::<E, DeclaredResponse<E::Response>>(request).await
}

async fn execute_request_as<E, S>(request: Request<E>) -> Result<ApiResponse<S::Output>, ApiError>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response>,
{
    let prepared = match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => prepared,
        OperationPreparation::Failed(failure) => {
            return complete_preparation_failure(failure).await;
        }
    };
    execute_prepared_operation::<E, S>(prepared, ExecutionContext::DIRECT)
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
    pub(crate) result: Result<ApiResponse<T>, ApiError>,
    pub(crate) final_status: Option<StatusCode>,
    pub(crate) attempts_started: u32,
    pub(crate) policy_retries: u32,
    pub(crate) recovered_authentication: bool,
}

pub(crate) async fn execute_pagination_page<E>(
    page: PreparedPaginationPage<E>,
    parent_operation_id: OperationId,
    page_index: u32,
) -> Result<ExecutedPage<<E::Response as ResponseBodySpec>::Output>, ApiError>
where
    E: EndpointSpec,
{
    let PreparedPaginationPage {
        core,
        endpoint,
        head,
        body_metadata,
    } = page;
    let started = tokio::time::Instant::now();
    let operation_id = core.allocate_operation_id(head.endpoint_id(), head.method())?;
    let context = ExecutionContext::page(parent_operation_id, page_index);
    let deadline = match OperationDeadline::new(started, head.timeout()) {
        Ok(deadline) => deadline,
        Err(_) => {
            let error = ApiError::configuration(
                head.endpoint_id().clone(),
                head.method().clone(),
                crate::error::ConfigurationErrorKind::InvalidValue,
            );
            let result = deliver_completion(
                &core,
                operation_id,
                context,
                head.endpoint(),
                head.method(),
                head.safe_url(),
                head.debug(),
                tokio::time::Instant::now().saturating_duration_since(started),
                None,
                CompletionOutcome::Success,
                0,
                0,
                false,
                Err(error),
            )
            .await;
            return Ok(ExecutedPage {
                result,
                final_status: None,
                attempts_started: 0,
                policy_retries: 0,
                recovered_authentication: false,
            });
        }
    };
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
        )
        .await,
    )
}

async fn execute_prepared_operation<E, S>(
    prepared: PreparedOperation<E>,
    context: ExecutionContext,
) -> ExecutedPage<S::Output>
where
    E: EndpointSpec,
    S: ResponseSelection<E::Response>,
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
    let policy_retries = 0_u32;
    let mut recovered_authentication = false;

    let result = async {
        let mut attempt_info = AttemptInfo::INITIAL;
        run_before_send(
            &core,
            operation_id,
            context,
            deadline,
            &head,
            &body_metadata,
            attempt_info,
        )
        .await?;

        check_deadline(deadline, head.endpoint_id(), head.method())?;
        let input = endpoint.into_body();
        let body = <E::Body as RequestBodySpec>::prepare(
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
            let attempt = send_attempt(
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
            .await?;
            let status = attempt.response.status();

            let Some(challenge_index) = attempt.applied_auth.challenge_index(status) else {
                break attempt.response;
            };

            let reusable = prepared_body.replayability() != Replayability::OneShot;
            let can_recover = !attempt_info.authentication_recovery
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
                        deadline.expires_at(),
                    )
                    .await;
                return Err(ApiError::authentication_rejected(failure));
            }

            // A response selected for recovery is dropped without polling its body.
            drop(attempt.response);
            attempt_info = AttemptInfo::AUTHENTICATION_RECOVERY;
            attempts_started = 2;
            if let Err(error) = run_before_send(
                &core,
                operation_id,
                context,
                deadline,
                &head,
                &body_metadata,
                attempt_info,
            )
            .await
            {
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
        };

        let status = response.status();
        let accepted = S::accepts_status(status);
        check_deadline(deadline, head.endpoint_id(), head.method())?;
        if !accepted {
            let failure = core
                .application_http()
                .terminal_status_managed(
                    head.endpoint_id().clone(),
                    head.method().clone(),
                    response,
                    deadline.expires_at(),
                )
                .await;
            return Err(ApiError::from_http_response(failure));
        }

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
        let output = execute_response::<E::Response, S>(
            ResponseInput {
                response,
                limit: core.response_body_limit(),
                stream_limit: core.stream_response_limit(),
                deadline,
                endpoint: head.endpoint_id().clone(),
                method: head.method().clone(),
            },
            &response_head,
        )
        .await?;
        Ok(ApiResponse::new(
            head.logical_url().clone(),
            status,
            version,
            headers,
            metadata,
            output,
        ))
    }
    .await;

    let fixed_at = tokio::time::Instant::now();
    let elapsed = deadline.elapsed_at(fixed_at);
    let result = deliver_completion(
        &core,
        operation_id,
        context,
        head.endpoint(),
        head.method(),
        head.safe_url(),
        head.debug(),
        elapsed,
        final_status,
        completion_outcome::<E::Response, S>(),
        attempts_started,
        policy_retries,
        recovered_authentication,
        result,
    )
    .await;
    ExecutedPage {
        result,
        final_status,
        attempts_started,
        policy_retries,
        recovered_authentication,
    }
}

async fn complete_preparation_failure<E>(
    failure: PreparationFailure,
) -> Result<ApiResponse<E>, ApiError> {
    let PreparationFailure {
        core,
        operation_id,
        meta,
        safe_url,
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
        &safe_url,
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

async fn run_before_send(
    core: &ClientCore,
    operation_id: OperationId,
    context: ExecutionContext,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    body_metadata: &RequestBodyMetadata,
    attempt: AttemptInfo,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let future = core.hooks().before_send(before_send_event(
        operation_id,
        context,
        head,
        body_metadata,
        attempt,
    ));
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    match deadline.run(future).await {
        Err(_) => Err(timeout_error(head.endpoint_id(), head.method())),
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            drop(error);
            Err(ApiError::classified(
                head.endpoint_id().clone(),
                head.method().clone(),
                ErrorCategory::Hook,
            ))
        }
    }
}

async fn run_response_head_hook(
    core: &ClientCore,
    deadline: OperationDeadline,
    endpoint_id: &EndpointId,
    event: ResponseHeadEvent<'_>,
) -> Result<(), ApiError> {
    check_deadline(deadline, endpoint_id, event.method)?;
    let endpoint_id = endpoint_id.clone();
    let method = event.method;
    let future = core.hooks().on_response_head(event);
    check_deadline(deadline, &endpoint_id, method)?;
    match deadline.run(future).await {
        Err(_) => Err(timeout_error(&endpoint_id, method)),
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            drop(error);
            Err(ApiError::classified(
                endpoint_id,
                method.clone(),
                ErrorCategory::Hook,
            ))
        }
    }
}

fn before_send_event<'a>(
    operation_id: OperationId,
    context: ExecutionContext,
    head: &'a crate::request::PreparedHead,
    body_metadata: &RequestBodyMetadata,
    attempt: AttemptInfo,
) -> BeforeSend<'a> {
    BeforeSend {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        safe_url: head.safe_url(),
        page_index: context.page_index,
        attempt,
        headers: head.sanitized_public_headers(),
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
    safe_url: &str,
    debug: DebugLevel,
    elapsed: std::time::Duration,
    final_status: Option<StatusCode>,
    success_outcome: CompletionOutcome,
    attempts_started: u32,
    policy_retries: u32,
    recovered_authentication: bool,
    result: Result<T, ApiError>,
) -> Result<T, ApiError> {
    let outcome = match &result {
        Ok(_) => success_outcome,
        Err(error) => CompletionOutcome::from_error_category(error.category()),
    };
    let event = CompletionEvent {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint,
        method,
        safe_url,
        page_index: context.page_index,
        attempts_started,
        policy_retries,
        recovered_authentication,
        elapsed,
        final_status,
        outcome,
    };
    core.debug_dispatcher(debug).completion(&event);
    core.hooks().on_complete(event).await;
    result
}

struct AttemptResponse {
    response: crate::http::application::ApplicationResponse,
    applied_auth: PreparedAuthPlan,
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
    acquire_rate_limit_authority(core, context, attempt, deadline, head).await?;
    let (applied_auth, finalized_auth) = run_auth(
        core,
        deadline,
        head,
        prepared_auth.finalize(core.provider_http(Some(deadline))),
    )
    .await?;

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
    let mut request = core.application_http().build_request(
        head.endpoint_id(),
        head.method(),
        head.logical_url(),
        head.public_headers(),
        native_body,
    )?;
    let request_event = before_send_event(operation_id, context, head, body_metadata, attempt);
    core.debug_dispatcher(head.debug())
        .request_ready(&request_event);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    finalized_auth.materialize(&mut request, head.endpoint_id(), head.method())?;

    let response = deadline
        .run(core.application_http().execute(
            head.endpoint_id(),
            head.method(),
            request,
            head.safe_url(),
            head.public_headers(),
            head.pagination_sensitive_headers(),
            body_metadata.kind,
            body_metadata.known_length(),
        ))
        .await
        .map_err(|_| timeout_error(head.endpoint_id(), head.method()))??;

    let status = response.status();
    *final_status = Some(status);
    let response_event = ResponseHeadEvent {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        safe_url: head.safe_url(),
        page_index: context.page_index,
        attempt,
        status,
        headers: SanitizedHeaders::new(response.headers()),
    };
    core.debug_dispatcher(head.debug())
        .response_head(&response_event);
    observe_rate_limit_response(
        core,
        context,
        attempt,
        deadline,
        head,
        status,
        response.headers(),
    )
    .await?;
    run_response_head_hook(core, deadline, head.endpoint_id(), response_event).await?;
    Ok(AttemptResponse {
        response,
        applied_auth,
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
        safe_url: head.safe_url(),
        host: head.rate_limit_host(),
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

async fn observe_rate_limit_response(
    core: &ClientCore,
    context: ExecutionContext,
    attempt: AttemptInfo,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    status: StatusCode,
    headers: &http::HeaderMap,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    let response = RateLimitResponse {
        request: RateLimitRequest {
            endpoint: head.endpoint(),
            method: head.method(),
            safe_url: head.safe_url(),
            host: head.rate_limit_host(),
            page_index: context.page_index,
            attempt,
            plan: head.rate_limit_plan(),
        },
        status,
        headers: SanitizedHeaders::new(headers),
        cooldown_cap: core.rate_limit_cooldown_cap(),
    };
    let future = core.rate_limiter().observe(response);
    check_deadline(deadline, head.endpoint_id(), head.method())?;
    deadline
        .run(future)
        .await
        .map_err(|_| timeout_error(head.endpoint_id(), head.method()))?
        .map_err(|error| {
            ApiError::rate_limit(head.endpoint_id().clone(), head.method().clone(), error)
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

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod tests;
