use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, StatusCode, Version};
use url::Url;

use crate::error::{ApiError, ErrorCategory};
use crate::http::application::ApplicationByteStream;

use super::ExecutionMeta;

/// A successful response head together with its bounded lazy body stream.
///
/// The endpoint operation is complete once this value is returned. Later
/// caller-driven polling is not governed by the endpoint deadline. Every
/// yielded chunk has passed the configured cumulative stream limit.
pub struct StreamResponse<M> {
    url: Url,
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    content_length: Option<u64>,
    metadata: ExecutionMeta,
    body: Option<ApplicationByteStream>,
    limit: u64,
    observed: u64,
    marker: PhantomData<fn() -> M>,
}

impl<M> StreamResponse<M> {
    #[allow(
        clippy::too_many_arguments,
        reason = "the stream response owns the exact normative response metadata"
    )]
    pub(crate) fn new(
        url: Url,
        status: StatusCode,
        version: Version,
        headers: HeaderMap,
        content_length: Option<u64>,
        metadata: ExecutionMeta,
        body: ApplicationByteStream,
        limit: u64,
    ) -> Self {
        Self {
            url,
            status,
            version,
            headers,
            content_length,
            metadata,
            body: Some(body),
            limit,
            observed: 0,
            marker: PhantomData,
        }
    }

    /// Returns the logical application URL.
    #[must_use]
    pub const fn url(&self) -> &Url {
        &self.url
    }

    /// Returns the successful HTTP status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the HTTP version observed on the response head.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    /// Returns the actual successful response headers.
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns the known decoded response length, when one remains available.
    #[must_use]
    pub const fn content_length(&self) -> Option<u64> {
        self.content_length
    }

    /// Returns Concord execution metadata for the stream-opening operation.
    #[must_use]
    pub const fn metadata(&self) -> &ExecutionMeta {
        &self.metadata
    }

    fn response_body_error(&self) -> ApiError {
        ApiError::classified(
            self.metadata.endpoint(),
            self.metadata.method().clone(),
            ErrorCategory::ResponseBody,
        )
    }
}

impl<M> Stream for StreamResponse<M> {
    type Item = Result<Bytes, ApiError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(body) = this.body.as_mut() else {
            return Poll::Ready(None);
        };
        match body.as_mut().poll_next(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.body = None;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.body = None;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(Some(Ok(bytes))) => {
                let Some(next) = this
                    .observed
                    .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                else {
                    let error = this.response_body_error();
                    this.body = None;
                    return Poll::Ready(Some(Err(error)));
                };
                if next > this.limit {
                    let error = this.response_body_error();
                    this.body = None;
                    return Poll::Ready(Some(Err(error)));
                }
                this.observed = next;
                Poll::Ready(Some(Ok(bytes)))
            }
        }
    }
}

impl<M> fmt::Debug for StreamResponse<M> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StreamResponse")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("header_count", &self.headers.len())
            .field("content_length", &self.content_length)
            .field("metadata", &self.metadata)
            .field("body", &"<response stream omitted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::poll_fn;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    struct ProbeStream {
        chunks: VecDeque<Result<Bytes, ApiError>>,
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    impl Stream for ProbeStream {
        type Item = Result<Bytes, ApiError>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(self.chunks.pop_front())
        }
    }

    impl Drop for ProbeStream {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    struct MarkerWithoutDebug;

    fn response(
        chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        limit: u64,
    ) -> StreamResponse<MarkerWithoutDebug> {
        let body: ApplicationByteStream = Box::pin(ProbeStream {
            chunks: chunks.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-stream-secret",
            http::HeaderValue::from_static("stream-debug-header-secret-d6f9"),
        );
        StreamResponse::new(
            Url::parse("https://stream.example.test/private?token=secret").unwrap(),
            StatusCode::OK,
            Version::HTTP_2,
            headers,
            None,
            ExecutionMeta::new("StreamApi::download", http::Method::GET, 0, false),
            body,
            limit,
        )
    }

    async fn next(
        response: &mut StreamResponse<MarkerWithoutDebug>,
    ) -> Option<Result<Bytes, ApiError>> {
        poll_fn(|context| Pin::new(&mut *response).poll_next(context)).await
    }

    #[test]
    fn debug_is_non_consuming_and_drop_cancels_the_owned_body() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let response = response(
            [Ok(Bytes::from_static(b"stream-debug-body-secret-a119"))],
            &polls,
            &dropped,
            64,
        );
        let debug = format!("{response:?}");
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        for forbidden in [
            "stream-debug-body-secret-a119",
            "stream-debug-header-secret-d6f9",
            "stream.example.test",
            "token=secret",
        ] {
            assert!(!debug.contains(forbidden));
        }
        drop(response);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cumulative_limit_drops_and_withholds_the_first_excess_chunk() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = response(
            [
                Ok(Bytes::from_static(b"abc")),
                Ok(Bytes::from_static(b"first-excess-body-secret")),
            ],
            &polls,
            &dropped,
            4,
        );
        assert_eq!(next(&mut response).await.unwrap().unwrap(), b"abc"[..]);
        let error = next(&mut response)
            .await
            .unwrap()
            .expect_err("first excess chunk must be withheld");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(next(&mut response).await.is_none());
        assert!(!format!("{error:?} {error}").contains("first-excess-body-secret"));
    }
}
