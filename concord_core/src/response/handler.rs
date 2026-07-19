use std::num::NonZeroUsize;

use bytes::{Bytes, BytesMut};
use http::header::CONTENT_TYPE;

use crate::codec::{DecodeBody, DecodeContext, MediaType};
use crate::error::{ApiError, ErrorCategory};
use crate::hooks::BoxFuture;
use crate::http::application::ApplicationResponse;
use crate::operation::OperationDeadline;
use crate::request::{
    BufferedResponse, BytesResponse, NoContentResponse, ResponseHandler, StreamResponseHandler,
};

use super::{ExecutionMeta, StreamResponse};

#[doc(hidden)]
pub struct ResponseInput {
    pub(crate) response: ApplicationResponse,
    pub(crate) limit: NonZeroUsize,
    pub(crate) stream_limit: NonZeroUsize,
    pub(crate) deadline: OperationDeadline,
    pub(crate) endpoint: &'static str,
    pub(crate) method: http::Method,
    pub(crate) logical_url: url::Url,
    pub(crate) recovered_authentication: bool,
}

#[doc(hidden)]
pub trait ExecuteResponse: ResponseHandler {
    fn handle(input: ResponseInput) -> BoxFuture<'static, Result<Self::Output, ApiError>>;
}

impl ExecuteResponse for BytesResponse {
    fn handle(input: ResponseInput) -> BoxFuture<'static, Result<Self::Output, ApiError>> {
        Box::pin(async move {
            preflight_length(&input)?;
            collect_bounded(input).await
        })
    }
}

impl<C> ExecuteResponse for BufferedResponse<C>
where
    C: DecodeBody,
{
    fn handle(input: ResponseInput) -> BoxFuture<'static, Result<Self::Output, ApiError>> {
        Box::pin(async move {
            preflight_length(&input)?;
            let content_type = input
                .response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let validation = C::validate_content_type(content_type.as_deref());
            input
                .deadline
                .check()
                .map_err(|_| timeout(input.endpoint, &input.method))?;
            validation.map_err(|error| {
                drop(error);
                ApiError::classified(
                    input.endpoint,
                    input.method.clone(),
                    ErrorCategory::ResponseContract,
                )
            })?;

            let endpoint = input.endpoint;
            let method = input.method.clone();
            let status = input.response.status();
            let deadline = input.deadline;
            let body = collect_bounded(input).await?;
            let decoded = C::decode(
                body,
                DecodeContext {
                    endpoint,
                    method: &method,
                    status,
                    content_type: content_type.as_deref(),
                },
            );
            deadline.check().map_err(|_| timeout(endpoint, &method))?;
            decoded.map_err(|error| {
                drop(error);
                ApiError::classified(endpoint, method, ErrorCategory::ResponseDecoding)
            })
        })
    }
}

impl ExecuteResponse for NoContentResponse {
    fn handle(input: ResponseInput) -> BoxFuture<'static, Result<Self::Output, ApiError>> {
        Box::pin(async move {
            input
                .deadline
                .check()
                .map_err(|_| timeout(input.endpoint, &input.method))?;
            drop(input.response);
            input
                .deadline
                .check()
                .map_err(|_| timeout(input.endpoint, &input.method))?;
            Ok(())
        })
    }
}

impl<M> ExecuteResponse for StreamResponseHandler<M>
where
    M: MediaType,
{
    fn handle(input: ResponseInput) -> BoxFuture<'static, Result<Self::Output, ApiError>> {
        Box::pin(async move {
            input
                .deadline
                .check()
                .map_err(|_| timeout(input.endpoint, &input.method))?;
            let content_length = input.response.content_length();
            let stream_limit = u64::try_from(input.stream_limit.get()).unwrap_or(u64::MAX);
            if content_length.is_some_and(|length| length > stream_limit) {
                return Err(ApiError::classified(
                    input.endpoint,
                    input.method,
                    ErrorCategory::ResponseBody,
                ));
            }

            let required = validate_stream_media_marker::<M>(input.endpoint, &input.method)?;
            let valid_media = input
                .response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<mime::Mime>().ok())
                .is_some_and(|actual| stream_media_matches(&required, &actual));
            input
                .deadline
                .check()
                .map_err(|_| timeout(input.endpoint, &input.method))?;
            if !valid_media {
                return Err(ApiError::classified(
                    input.endpoint,
                    input.method,
                    ErrorCategory::ResponseContract,
                ));
            }

            let status = input.response.status();
            let version = input.response.version();
            let headers = input.response.headers().clone();
            let metadata = ExecutionMeta::new(
                input.endpoint,
                input.method.clone(),
                0,
                input.recovered_authentication,
            );
            let body = input.response.into_byte_stream();
            input
                .deadline
                .check()
                .map_err(|_| timeout(input.endpoint, &input.method))?;
            Ok(StreamResponse::new(
                input.logical_url,
                status,
                version,
                headers,
                content_length,
                metadata,
                body,
                stream_limit,
            ))
        })
    }
}

pub(crate) fn validate_stream_media_marker<M>(
    endpoint: &'static str,
    method: &http::Method,
) -> Result<mime::Mime, ApiError>
where
    M: MediaType,
{
    let parsed = M::MEDIA_TYPE.parse::<mime::Mime>().map_err(|_| {
        ApiError::classified(endpoint, method.clone(), ErrorCategory::ResponseContract)
    })?;
    if parsed.type_() == mime::STAR || parsed.subtype() == mime::STAR {
        return Err(ApiError::classified(
            endpoint,
            method.clone(),
            ErrorCategory::ResponseContract,
        ));
    }
    Ok(parsed)
}

fn stream_media_matches(required: &mime::Mime, actual: &mime::Mime) -> bool {
    required.type_() == actual.type_()
        && required.subtype() == actual.subtype()
        && required
            .params()
            .all(|(name, value)| actual.get_param(name) == Some(value))
}

fn timeout(endpoint: &'static str, method: &http::Method) -> ApiError {
    ApiError::classified(endpoint, method.clone(), ErrorCategory::Timeout)
}

fn preflight_length(input: &ResponseInput) -> Result<(), ApiError> {
    input
        .deadline
        .check()
        .map_err(|_| timeout(input.endpoint, &input.method))?;
    if input
        .response
        .content_length()
        .is_some_and(|length| length > input.limit.get() as u64)
    {
        return Err(ApiError::classified(
            input.endpoint,
            input.method.clone(),
            ErrorCategory::ResponseBody,
        ));
    }
    Ok(())
}

async fn collect_bounded(input: ResponseInput) -> Result<Bytes, ApiError> {
    let endpoint = input.endpoint;
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
                        endpoint,
                        body_method.clone(),
                        ErrorCategory::ResponseBody,
                    ));
                };
                if next_length > limit {
                    return Err(ApiError::classified(
                        endpoint,
                        body_method.clone(),
                        ErrorCategory::ResponseBody,
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            Ok::<Bytes, ApiError>(body.freeze())
        })
        .await
        .map_err(|_| timeout(endpoint, &method))?
}
