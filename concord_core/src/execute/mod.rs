//! One statically typed production engine for generated requests.

use std::future::IntoFuture;

use http::{Method, StatusCode};

use crate::client::ClientCore;
use crate::error::{ApiError, ErrorCategory};
use crate::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionOutcome, DebugLevel, ResponseHeadEvent,
};
use crate::operation::{OperationDeadline, OperationId};
use crate::redaction::SanitizedHeaders;
use crate::request::{
    BodySpec, EndpointSpec, OperationPreparation, PreparationFailure, PreparedOperation, Request,
    ResponseHandler, ResponseRequest, prepare_operation,
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
    let PreparedOperation {
        core,
        endpoint,
        operation_id,
        deadline,
        head,
        body_metadata,
    } = prepared;
    let mut final_status = None;

    let result = async {
        run_before_send(&core, operation_id, deadline, &head, &body_metadata).await?;

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
        let materialized_metadata = body_metadata.with_known_length(known_body_length);

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

        let request = core.application_http().build_request(
            head.endpoint(),
            head.method(),
            head.logical_url(),
            head.public_headers(),
            native_body,
        )?;
        let request_event = before_send_event(operation_id, &head, &materialized_metadata);
        core.debug_dispatcher(head.debug())
            .request_ready(&request_event);
        check_deadline(deadline, head.endpoint(), head.method())?;

        let response = deadline
            .run(core.application_http().execute(
                head.endpoint(),
                head.method(),
                request,
                head.safe_url(),
                head.public_headers(),
                materialized_metadata.kind(),
                materialized_metadata.known_length(),
            ))
            .await
            .map_err(|_| timeout_error(head.endpoint(), head.method()))??;

        let status = response.status();
        final_status = Some(status);
        let response_event = ResponseHeadEvent {
            operation_id,
            parent_operation_id: None,
            endpoint: head.endpoint(),
            method: head.method(),
            safe_url: head.safe_url(),
            page_index: 0,
            recovery_index: 0,
            status,
            headers: SanitizedHeaders::new(response.headers()),
        };
        core.debug_dispatcher(head.debug())
            .response_head(&response_event);
        run_response_head_hook(&core, deadline, response_event).await?;

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
            deadline,
            endpoint: head.endpoint(),
            method: head.method().clone(),
        })
        .await?;
        Ok(DecodedResponse::new(
            head.logical_url().clone(),
            status,
            version,
            headers,
            ExecutionMeta::new(head.endpoint(), head.method().clone(), 0, false),
            output,
        ))
    }
    .await;

    let fixed_at = tokio::time::Instant::now();
    let elapsed = deadline.elapsed_at(fixed_at);
    deliver_completion(
        &core,
        operation_id,
        head.endpoint(),
        head.method(),
        head.safe_url(),
        head.debug(),
        elapsed,
        final_status,
        result,
    )
    .await
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
        meta.endpoint(),
        meta.method(),
        &safe_url,
        debug,
        elapsed,
        None,
        Err(error),
    )
    .await
}

async fn run_before_send(
    core: &ClientCore,
    operation_id: OperationId,
    deadline: OperationDeadline,
    head: &crate::request::PreparedHead,
    body_metadata: &crate::request::BodyMetadata,
) -> Result<(), ApiError> {
    check_deadline(deadline, head.endpoint(), head.method())?;
    let future = core
        .hooks()
        .before_send(before_send_event(operation_id, head, body_metadata));
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
    head: &'a crate::request::PreparedHead,
    body_metadata: &crate::request::BodyMetadata,
) -> BeforeSend<'a> {
    BeforeSend {
        operation_id,
        parent_operation_id: None,
        endpoint: head.endpoint(),
        method: head.method(),
        safe_url: head.safe_url(),
        page_index: 0,
        recovery_index: 0,
        headers: SanitizedHeaders::new(head.public_headers()),
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
    endpoint: &'static str,
    method: &Method,
    safe_url: &str,
    debug: DebugLevel,
    elapsed: std::time::Duration,
    final_status: Option<StatusCode>,
    result: Result<T, ApiError>,
) -> Result<T, ApiError> {
    let outcome = match &result {
        Ok(_) => CompletionOutcome::Success,
        Err(error) => CompletionOutcome::from_error_category(error.category()),
    };
    let event = CompletionEvent {
        operation_id,
        parent_operation_id: None,
        endpoint,
        method,
        safe_url,
        page_index: 0,
        recovered_authentication: false,
        elapsed,
        final_status,
        outcome,
    };
    core.debug_dispatcher(debug).completion(&event);
    core.hooks().on_complete(event).await;
    result
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
mod tests;
