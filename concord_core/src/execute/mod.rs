//! One statically typed production engine for generated requests.

use std::future::IntoFuture;

use http::{Method, StatusCode};

use crate::auth::PreparedAuthPlan;
use crate::body::Replayability;
use crate::client::ClientCore;
use crate::error::{ApiError, ErrorCategory};
use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel, ResponseHeadEvent,
};
use crate::operation::{OperationDeadline, OperationId};
use crate::rate_limit::{RateLimitRequest, RateLimitResponse};
use crate::redaction::SanitizedHeaders;
use crate::request::{
    BodySpec, EndpointSpec, OperationPreparation, PreparationFailure, PreparedOperation,
    PreparedPaginationPage, Request, ResponseHandler, ResponseRequest, prepare_operation,
};
use crate::response::{DecodedResponse, ExecuteResponse, ExecutionMeta, ResponseInput};

#[doc(hidden)]
impl<E> IntoFuture for Request<E>
where
    E: EndpointSpec,
    E::Response: ExecuteResponse,
{
    type Output = Result<<E::Response as ResponseHandler>::Output, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { execute_request(self).await.map(DecodedResponse::into_value) })
    }
}

#[doc(hidden)]
impl<E> IntoFuture for ResponseRequest<E>
where
    E: EndpointSpec,
    E::Response: ExecuteResponse,
{
    type Output = Result<DecodedResponse<<E::Response as ResponseHandler>::Output>, ApiError>;
    type IntoFuture = BoxFuture<'static, Self::Output>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(execute_request(self.request))
    }
}

async fn execute_request<E>(
    request: Request<E>,
) -> Result<DecodedResponse<<E::Response as ResponseHandler>::Output>, ApiError>
where
    E: EndpointSpec,
    E::Response: ExecuteResponse,
{
    let prepared = match prepare_operation(request).await? {
        OperationPreparation::Ready(prepared) => prepared,
        OperationPreparation::Failed(failure) => {
            return complete_preparation_failure(failure).await;
        }
    };
    execute_prepared_operation(prepared, ExecutionContext::DIRECT)
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
    pub(crate) result: Result<DecodedResponse<T>, ApiError>,
    pub(crate) final_status: Option<StatusCode>,
    pub(crate) recovered_authentication: bool,
}

pub(crate) async fn execute_pagination_page<E>(
    page: PreparedPaginationPage<E>,
    parent_operation_id: OperationId,
    page_index: u32,
) -> Result<ExecutedPage<<E::Response as ResponseHandler>::Output>, ApiError>
where
    E: EndpointSpec,
    E::Response: ExecuteResponse,
{
    let PreparedPaginationPage {
        core,
        endpoint,
        head,
        body_metadata,
    } = page;
    let started = tokio::time::Instant::now();
    let operation_id = core.allocate_operation_id(head.endpoint(), head.method())?;
    let context = ExecutionContext::page(parent_operation_id, page_index);
    let deadline = match OperationDeadline::new(started, head.timeout()) {
        Ok(deadline) => deadline,
        Err(_) => {
            let error = ApiError::configuration(
                head.endpoint(),
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
                false,
                Err(error),
            )
            .await;
            return Ok(ExecutedPage {
                result,
                final_status: None,
                recovered_authentication: false,
            });
        }
    };
    Ok(execute_prepared_operation(
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
    .await)
}

async fn execute_prepared_operation<E>(
    prepared: PreparedOperation<E>,
    context: ExecutionContext,
) -> ExecutedPage<<E::Response as ResponseHandler>::Output>
where
    E: EndpointSpec,
    E::Response: ExecuteResponse,
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
    let mut recovered_authentication = false;

    let result = async {
        run_before_send(
            &core,
            operation_id,
            context,
            deadline,
            &head,
            &body_metadata,
            0,
        )
        .await?;

        check_deadline(deadline, head.endpoint(), head.method())?;
        let materialized = endpoint.into_body().materialize(
            head.endpoint(),
            head.method(),
            core.request_body_limit(),
        );
        check_deadline(deadline, head.endpoint(), head.method())?;
        let mut prepared_body = materialized?;
        debug_assert_eq!(prepared_body.kind(), body_metadata.kind());
        let known_body_length = prepared_body.known_length();
        let materialized_metadata = body_metadata.clone().with_known_length(known_body_length);

        let mut prepared_auth = run_auth(
            &core,
            deadline,
            &head,
            head.auth().preflight(core.provider_http(Some(deadline))),
        )
        .await?;
        let mut recovery_index = 0_u8;
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
                recovery_index,
                &mut final_status,
                context,
            )
            .await?;
            let status = attempt.response.status();

            let Some(challenge_index) = attempt.applied_auth.challenge_index(status) else {
                break attempt.response;
            };

            let reusable = prepared_body.replayability() != Replayability::OneShot;
            let can_recover = recovery_index == 0
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
                        head.endpoint(),
                        head.method().clone(),
                        attempt.response,
                        deadline.expires_at(),
                    )
                    .await;
                return Err(ApiError::authentication_rejected(failure));
            }

            // A response selected for recovery is dropped without polling its body.
            drop(attempt.response);
            if let Err(error) = run_before_send(
                &core,
                operation_id,
                context,
                deadline,
                &head,
                &body_metadata,
                1,
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
                    head.endpoint(),
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
            recovery_index = 1;
            recovered_authentication = true;
        };

        let status = response.status();
        if !status.is_success() {
            let failure = core
                .application_http()
                .terminal_status_managed(
                    head.endpoint(),
                    head.method().clone(),
                    response,
                    deadline.expires_at(),
                )
                .await;
            return Err(ApiError::from_http_response(failure));
        }

        let version = response.version();
        let headers = response.headers().clone();
        let output = <E::Response as ExecuteResponse>::handle(ResponseInput {
            response,
            limit: core.response_body_limit(),
            stream_limit: core.stream_response_limit(),
            deadline,
            endpoint: head.endpoint(),
            method: head.method().clone(),
            logical_url: head.logical_url().clone(),
            recovered_authentication,
        })
        .await?;
        Ok(DecodedResponse::new(
            head.logical_url().clone(),
            status,
            version,
            headers,
            ExecutionMeta::new(
                head.endpoint(),
                head.method().clone(),
                context.page_index,
                recovered_authentication,
            ),
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
        E::Response::COMPLETION_OUTCOME,
        recovered_authentication,
        result,
    )
    .await;
    ExecutedPage {
        result,
        final_status,
        recovered_authentication,
    }
}

async fn complete_preparation_failure<E>(
    failure: PreparationFailure,
) -> Result<DecodedResponse<E>, ApiError> {
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
    body_metadata: &crate::request::BodyMetadata,
    recovery_index: u8,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint(), head.method())?;
    let future = core.hooks().before_send(before_send_event(
        operation_id,
        context,
        head,
        body_metadata,
        recovery_index,
    ));
    check_deadline(deadline, head.endpoint(), head.method())?;
    match deadline.run(future).await {
        Err(_) => Err(timeout_error(head.endpoint(), head.method())),
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            drop(error);
            Err(ApiError::classified(
                head.endpoint(),
                head.method().clone(),
                ErrorCategory::Hook,
            ))
        }
    }
}

async fn run_response_head_hook(
    core: &ClientCore,
    deadline: OperationDeadline,
    event: ResponseHeadEvent<'_>,
) -> Result<(), ApiError> {
    check_deadline(deadline, event.endpoint, event.method)?;
    let endpoint = event.endpoint;
    let method = event.method;
    let future = core.hooks().on_response_head(event);
    check_deadline(deadline, endpoint, method)?;
    deadline
        .run(future)
        .await
        .map_err(|_| timeout_error(endpoint, method))
}

fn before_send_event<'a>(
    operation_id: OperationId,
    context: ExecutionContext,
    head: &'a crate::request::PreparedHead,
    body_metadata: &crate::request::BodyMetadata,
    recovery_index: u8,
) -> BeforeSend<'a> {
    BeforeSend {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        safe_url: head.safe_url(),
        page_index: context.page_index,
        recovery_index,
        headers: head.sanitized_public_headers(),
        body_kind: body_metadata.kind(),
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
    endpoint: &'static str,
    method: &Method,
    safe_url: &str,
    debug: DebugLevel,
    elapsed: std::time::Duration,
    final_status: Option<StatusCode>,
    success_outcome: CompletionOutcome,
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
    body_metadata: &crate::request::BodyMetadata,
    prepared_auth: PreparedAuthPlan,
    recovery_index: u8,
    final_status: &mut Option<StatusCode>,
    context: ExecutionContext,
) -> Result<AttemptResponse, ApiError> {
    acquire_rate_limit_authority(core, context, deadline, head).await?;
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
                head.endpoint(),
                head.method().clone(),
                ErrorCategory::RequestBody,
            )
        })?;
    let mut request = core.application_http().build_request(
        head.endpoint(),
        head.method(),
        head.logical_url(),
        head.public_headers(),
        native_body,
    )?;
    let request_event =
        before_send_event(operation_id, context, head, body_metadata, recovery_index);
    core.debug_dispatcher(head.debug())
        .request_ready(&request_event);
    check_deadline(deadline, head.endpoint(), head.method())?;
    finalized_auth.materialize(&mut request, head.endpoint(), head.method())?;

    let response = deadline
        .run(core.application_http().execute(
            head.endpoint(),
            head.method(),
            request,
            head.safe_url(),
            head.public_headers(),
            head.pagination_sensitive_headers(),
            body_metadata.kind(),
            body_metadata.known_length(),
        ))
        .await
        .map_err(|_| timeout_error(head.endpoint(), head.method()))??;

    let status = response.status();
    *final_status = Some(status);
    let response_event = ResponseHeadEvent {
        operation_id,
        parent_operation_id: context.parent_operation_id,
        endpoint: head.endpoint(),
        method: head.method(),
        safe_url: head.safe_url(),
        page_index: context.page_index,
        recovery_index,
        status,
        headers: SanitizedHeaders::new(response.headers()),
    };
    core.debug_dispatcher(head.debug())
        .response_head(&response_event);
    run_response_head_hook(core, deadline, response_event).await?;
    observe_rate_limit_response(core, context, head, status, response.headers()).await;
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
        .map_err(|_| timeout_error(head.endpoint(), head.method()))?
        .map_err(|error| auth_api_error(head, error))
}

fn auth_api_error(head: &crate::request::PreparedHead, error: crate::auth::AuthError) -> ApiError {
    ApiError::authentication(head.endpoint(), head.method().clone(), error)
}

async fn acquire_rate_limit_authority(
    core: &ClientCore,
    context: ExecutionContext,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint(), head.method())?;
    let request = RateLimitRequest {
        endpoint: head.endpoint(),
        method: head.method(),
        safe_url: head.safe_url(),
        host: head.safe_host(),
        page_index: context.page_index,
        plan: head.rate_limit_plan(),
    };
    let future = core.rate_limiter().acquire(request);
    check_deadline(deadline, head.endpoint(), head.method())?;
    deadline
        .run(future)
        .await
        .map_err(|_| timeout_error(head.endpoint(), head.method()))?
        .map(|_permit| ())
        .map_err(|error| ApiError::rate_limit(head.endpoint(), head.method().clone(), error))
}

async fn observe_rate_limit_response(
    core: &ClientCore,
    context: ExecutionContext,
    head: &crate::request::PreparedHead,
    status: StatusCode,
    headers: &http::HeaderMap,
) {
    let response = RateLimitResponse {
        request: RateLimitRequest {
            endpoint: head.endpoint(),
            method: head.method(),
            safe_url: head.safe_url(),
            host: head.safe_host(),
            page_index: context.page_index,
            plan: head.rate_limit_plan(),
        },
        status,
        headers: SanitizedHeaders::new(headers),
    };
    crate::rate_limit::observe_with_cooldown_cap(
        core.rate_limiter(),
        response,
        core.rate_limit_cooldown_cap(),
    )
    .await;
}

fn check_deadline(
    deadline: OperationDeadline,
    endpoint: &'static str,
    method: &Method,
) -> Result<(), ApiError> {
    deadline
        .check()
        .map_err(|_| timeout_error(endpoint, method))
}

fn timeout_error(endpoint: &'static str, method: &Method) -> ApiError {
    ApiError::classified(endpoint, method.clone(), ErrorCategory::Timeout)
}

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod tests;
