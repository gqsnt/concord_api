use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;

use crate::error::ApiError;
use crate::http::application::ApplicationByteStream;

/// A bounded media-independent lazy successful response body.
///
/// The endpoint operation is complete once this value is returned. Later
/// caller-driven polling is not governed by the endpoint deadline. Every
/// yielded chunk has passed the configured cumulative stream limit, measured
/// post-decompression. Unknown chunks remain unpolled at return, and the first chunk that
/// would exceed the cumulative limit is withheld. Native read failures become
/// safe [`ApiError`] items. Dropping the stream cancels and drops the owned
/// native body; later failures do not emit another completion event.
///
/// `ByteStream` contains neither response metadata nor a media marker. Its
/// `Debug` implementation does not inspect the body or any response data.
pub struct ByteStream {
    body: Option<ApplicationByteStream>,
}

impl ByteStream {
    pub(crate) const fn new(body: ApplicationByteStream) -> Self {
        Self { body: Some(body) }
    }
}

impl Stream for ByteStream {
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
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
        }
    }
}

impl fmt::Debug for ByteStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ByteStream")
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
    use crate::endpoint::EndpointId;
    use crate::error::ErrorCategory;

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

    fn response(
        chunks: impl IntoIterator<Item = Result<Bytes, ApiError>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        limit: u64,
    ) -> ByteStream {
        let body: ApplicationByteStream = Box::pin(ProbeStream {
            chunks: chunks.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
        });
        ByteStream::new(crate::http::application::bounded_byte_stream(
            body,
            EndpointId::from_static("stream.download"),
            http::Method::GET,
            limit,
        ))
    }

    async fn next(response: &mut ByteStream) -> Option<Result<Bytes, ApiError>> {
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
        for forbidden in ["stream-debug-body-secret-a119", "StreamApi::download"] {
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
