use std::num::NonZeroUsize;

use bytes::{Bytes, BytesMut};

use crate::endpoint::EndpointId;
use crate::error::{ApiError, DispatchKnowledge, ErrorCategory, FailureStage};
use crate::http::application::ApplicationResponse;
use crate::operation::OperationDeadline;

use super::spec::ResponseSelection;
use super::stream::StreamObservation;
use super::{ByteStream, ResponseBodyErrorKind, ResponseHead, ResponseRepresentation};

pub struct ResponseInput {
    pub(crate) response: ApplicationResponse,
    pub(crate) limit: NonZeroUsize,
    pub(crate) stream_limit: NonZeroUsize,
    pub(crate) deadline: OperationDeadline,
    pub(crate) endpoint: EndpointId,
    pub(crate) method: http::Method,
    pub(crate) stream_observation: Option<StreamObservation>,
}

pub(crate) async fn execute_response<R, DeclaredOutput, S>(
    input: ResponseInput,
    head: &ResponseHead<'_>,
) -> Result<(S::Output, usize), ApiError>
where
    R: ResponseRepresentation<DeclaredOutput>,
    DeclaredOutput: Send + 'static,
    S: ResponseSelection<R, DeclaredOutput>,
{
    <S::Driver as super::spec::private::Sealed>::preflight(&input)?;

    let validation = S::validate(head);
    input
        .deadline
        .check()
        .map_err(|_| conversion_timeout(&input.endpoint, &input.method))?;
    validation
        .map_err(|error| response_spec_error(&input.endpoint, &input.method, error.kind()))?;

    let deadline = input.deadline;
    let endpoint = input.endpoint.clone();
    let method = input.method.clone();
    let body = <S::Driver as super::spec::private::Sealed>::acquire(input).await?;
    let acquired_body_bytes =
        <S::Driver as super::spec::private::Sealed>::acquired_body_bytes(&body);
    let output = S::from_body(body, head);
    deadline
        .check()
        .map_err(|_| conversion_timeout(&endpoint, &method))?;
    let output = output.map_err(|error| response_spec_error(&endpoint, &method, error.kind()))?;
    Ok((output, acquired_body_bytes))
}

pub(crate) fn preflight_buffered(input: &ResponseInput) -> Result<(), ApiError> {
    preflight_length(input, input.limit)
}

pub(crate) fn preflight_streaming(input: &ResponseInput) -> Result<(), ApiError> {
    preflight_length(input, input.stream_limit)
}

pub(crate) fn preflight_discard(input: &ResponseInput) -> Result<(), ApiError> {
    preflight_length(input, input.limit)
}

pub(crate) async fn acquire_buffered(input: ResponseInput) -> Result<Bytes, ApiError> {
    collect_bounded(input).await
}

pub(crate) async fn acquire_streaming(mut input: ResponseInput) -> Result<ByteStream, ApiError> {
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
    Ok(match input.stream_observation.take() {
        Some(observation) => ByteStream::observed(body, observation),
        None => ByteStream::new(body),
    })
}

pub(crate) async fn acquire_discard(input: ResponseInput) -> Result<(), ApiError> {
    collect_bounded(input).await.map(drop)
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
    ApiError::classified(endpoint.clone(), method.clone(), category).with_stage(
        FailureStage::ResponseConversion,
        DispatchKnowledge::ResponseObserved,
    )
}

fn timeout(endpoint: &EndpointId, method: &http::Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout).with_stage(
        FailureStage::ResponseAcquisition,
        DispatchKnowledge::ResponseObserved,
    )
}

fn conversion_timeout(endpoint: &EndpointId, method: &http::Method) -> ApiError {
    ApiError::classified(endpoint.clone(), method.clone(), ErrorCategory::Timeout).with_stage(
        FailureStage::ResponseConversion,
        DispatchKnowledge::ResponseObserved,
    )
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
        )
        .with_stage(
            FailureStage::ResponseAcquisition,
            DispatchKnowledge::ResponseObserved,
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
                    )
                    .with_stage(
                        FailureStage::ResponseAcquisition,
                        DispatchKnowledge::ResponseObserved,
                    ));
                };
                if next_length > limit {
                    return Err(ApiError::classified(
                        endpoint.clone(),
                        body_method.clone(),
                        ErrorCategory::ResponseBody,
                    )
                    .with_stage(
                        FailureStage::ResponseAcquisition,
                        DispatchKnowledge::ResponseObserved,
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            Ok::<Bytes, ApiError>(body.freeze())
        })
        .await
        .map_err(|_| timeout(&timeout_endpoint, &method))?
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll, Waker};

    use bytes::Bytes;

    use super::*;
    use crate::error::ErrorCategory;
    use crate::http::response_progress::test_support::{empty_chunks, response as native_response};
    use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};

    fn input(
        chunks: impl IntoIterator<Item = Result<Bytes, io::Error>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        limit: usize,
    ) -> ResponseInput {
        let endpoint = EndpointId::from_static("buffered.success");
        let method = http::Method::GET;
        let limit = NonZeroUsize::new(limit).expect("positive response limit");
        ResponseInput {
            response: ApplicationResponse::from_native_for_test(
                native_response(chunks, polls, dropped),
                endpoint.clone(),
                method.clone(),
            ),
            limit,
            stream_limit: limit,
            deadline: OperationDeadline::new(tokio::time::Instant::now(), None)
                .expect("disabled deadline"),
            endpoint,
            method,
            stream_observation: None,
        }
    }

    #[tokio::test]
    async fn buffered_success_suppresses_empty_chunks() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let result = acquire_buffered(input(
            [
                Ok(Bytes::new()),
                Ok(Bytes::from_static(b"buffered")),
                Ok(Bytes::new()),
            ],
            &polls,
            &dropped,
            64,
        ))
        .await
        .expect("buffered response");

        assert_eq!(result, Bytes::from_static(b"buffered"));
        assert_eq!(polls.load(Ordering::SeqCst), 4);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn buffered_success_yields_executor_after_ready_empty_budget() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN);
        chunks.push(Ok(Bytes::from_static(b"deferred")));
        let mut acquisition = Box::pin(acquire_buffered(input(chunks, &polls, &dropped, 64)));
        let mut context = Context::from_waker(Waker::noop());

        assert!(matches!(
            acquisition.as_mut().poll(&mut context),
            Poll::Pending
        ));
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
    }

    #[tokio::test]
    async fn buffered_success_exact_empty_limit_succeeds() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS);
        chunks.push(Ok(Bytes::from_static(b"exact")));

        let result = acquire_buffered(input(chunks, &polls, &dropped, 64))
            .await
            .expect("exact empty-chunk limit");
        assert_eq!(result, Bytes::from_static(b"exact"));
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 2);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn buffered_success_first_excess_fails() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));

        let error = acquire_buffered(input(
            empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1),
            &polls,
            &dropped,
            64,
        ))
        .await
        .expect_err("first excess empty response chunk");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 1);
        assert!(dropped.load(Ordering::SeqCst));
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
        assert!(!rendered.contains("empty chunk"));
    }

    #[tokio::test]
    async fn buffered_success_nonempty_byte_limit_is_unchanged() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));

        let error = acquire_buffered(input(
            [
                Ok(Bytes::from_static(b"abc")),
                Ok(Bytes::from_static(b"de")),
            ],
            &polls,
            &dropped,
            4,
        ))
        .await
        .expect_err("first nonempty byte excess");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(dropped.load(Ordering::SeqCst));
    }
}
