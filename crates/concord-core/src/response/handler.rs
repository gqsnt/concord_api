use std::num::NonZeroUsize;

use bytes::{Bytes, BytesMut};

use crate::endpoint::EndpointId;
use crate::error::{ApiError, ErrorCategory};
use crate::http::application::ApplicationResponse;
use crate::operation::OperationDeadline;

use super::{ByteStream, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead};

pub struct ResponseInput {
    pub(crate) response: ApplicationResponse,
    pub(crate) limit: NonZeroUsize,
    pub(crate) stream_limit: NonZeroUsize,
    pub(crate) deadline: OperationDeadline,
    pub(crate) endpoint: EndpointId,
    pub(crate) method: http::Method,
}

pub(crate) async fn execute_response<R>(
    input: ResponseInput,
    head: &ResponseHead<'_>,
) -> Result<R::Output, ApiError>
where
    R: ResponseBodySpec,
{
    <R::Mode as super::spec::private::Sealed>::preflight(&input)?;

    let validation = R::validate(head);
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))?;
    validation
        .map_err(|error| response_spec_error(&input.endpoint, &input.method, error.kind()))?;

    let deadline = input.deadline;
    let endpoint = input.endpoint.clone();
    let method = input.method.clone();
    let body = <R::Mode as super::spec::private::Sealed>::acquire(input).await?;
    let output = R::from_body(body, head);
    deadline.check().map_err(|_| timeout(&endpoint, &method))?;
    output.map_err(|error| response_spec_error(&endpoint, &method, error.kind()))
}

pub(crate) fn preflight_buffered(input: &ResponseInput) -> Result<(), ApiError> {
    preflight_length(input, input.limit)
}

pub(crate) fn preflight_streaming(input: &ResponseInput) -> Result<(), ApiError> {
    preflight_length(input, input.stream_limit)
}

pub(crate) fn preflight_discard(input: &ResponseInput) -> Result<(), ApiError> {
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))
}

pub(crate) async fn acquire_buffered(input: ResponseInput) -> Result<Bytes, ApiError> {
    collect_bounded(input).await
}

pub(crate) async fn acquire_streaming(input: ResponseInput) -> Result<ByteStream, ApiError> {
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))?;
    let limit = u64::try_from(input.stream_limit.get()).unwrap_or(u64::MAX);
    let body = input.response.into_byte_stream(limit);
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))?;
    Ok(ByteStream::new(body))
}

pub(crate) async fn acquire_discard(input: ResponseInput) -> Result<(), ApiError> {
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))?;
    drop(input.response);
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))?;
    Ok(())
}

fn response_spec_error(
    endpoint: &EndpointId,
    method: &http::Method,
    kind: ResponseBodyErrorKind,
) -> ApiError {
    let category = match kind {
        ResponseBodyErrorKind::InvalidMediaType | ResponseBodyErrorKind::InvalidOutput => {
            ErrorCategory::ResponseContract
        }
        ResponseBodyErrorKind::Decode => ErrorCategory::ResponseDecoding,
        ResponseBodyErrorKind::Internal => ErrorCategory::Internal,
    };
    ApiError::classified(endpoint.clone(), method.clone(), category)
}

fn timeout(endpoint: &EndpointId, method: &http::Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout)
}

fn preflight_length(input: &ResponseInput, limit: NonZeroUsize) -> Result<(), ApiError> {
    input
        .deadline
        .check()
        .map_err(|_| timeout(&input.endpoint, &input.method))?;
    if input
        .response
        .content_length()
        .is_some_and(|length| length > u64::try_from(limit.get()).unwrap_or(u64::MAX))
    {
        return Err(ApiError::classified(
            input.endpoint.clone(),
            input.method.clone(),
            ErrorCategory::ResponseBody,
        ));
    }
    Ok(())
}

async fn collect_bounded(input: ResponseInput) -> Result<Bytes, ApiError> {
    let endpoint = input.endpoint;
    let timeout_endpoint = endpoint.clone();
    let method = input.method;
    let body_method = method.clone();
    let limit = input.limit.get();
    let mut response = input.response;
    input
        .deadline
        .run(async move {
            let mut body = BytesMut::new();
            while let Some(chunk) = response.chunk().await? {
                let Some(next_length) = body.len().checked_add(chunk.len()) else {
                    return Err(ApiError::classified(
                        endpoint.clone(),
                        body_method.clone(),
                        ErrorCategory::ResponseBody,
                    ));
                };
                if next_length > limit {
                    return Err(ApiError::classified(
                        endpoint.clone(),
                        body_method.clone(),
                        ErrorCategory::ResponseBody,
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            Ok::<Bytes, ApiError>(body.freeze())
        })
        .await
        .map_err(|_| timeout(&timeout_endpoint, &method))?
}
