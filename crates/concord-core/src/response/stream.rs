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
/// post-decompression. Empty transport chunks are suppressed as framing
/// artifacts and never become application items. A hard
/// [`MAX_EMPTY_RESPONSE_CHUNKS`](crate::contracts::MAX_EMPTY_RESPONSE_CHUNKS)
/// ceiling bounds total non-byte progress, while bounded per-poll empty-chunk
/// work yields cooperatively back to the executor.
///
/// Callers that require a polling or whole-download deadline must wrap
/// consumption in their runtime's own timeout or cancellation primitive.
/// Unknown chunks remain unpolled at return, and the first nonempty chunk that
/// would exceed the cumulative byte limit is withheld. Native read failures
/// become safe [`ApiError`] items. Dropping the stream cancels and drops the
/// owned native body; later failures do not emit another completion event.
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
    use std::task::{Wake, Waker};

    use super::*;
    use crate::endpoint::EndpointId;
    use crate::error::ErrorCategory;
    use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_POLL};

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

    #[derive(Default)]
    struct WakeCounter {
        wakes: AtomicUsize,
    }

    impl Wake for WakeCounter {
        fn wake(self: Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.wakes.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn empty_chunks(count: usize) -> Vec<Result<Bytes, ApiError>> {
        std::iter::repeat_with(|| Ok(Bytes::new()))
            .take(count)
            .collect()
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

    #[tokio::test]
    async fn empty_response_chunk_is_not_yielded() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = response(
            [
                Ok(Bytes::new()),
                Ok(Bytes::from_static(b"application-data")),
            ],
            &polls,
            &dropped,
            64,
        );

        assert_eq!(
            next(&mut response).await.unwrap().unwrap(),
            b"application-data"[..]
        );
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn single_terminal_empty_chunk_is_tolerated() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = response([Ok(Bytes::new())], &polls, &dropped, 64);

        assert!(next(&mut response).await.is_none());
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn empty_chunks_before_nonempty_data_are_suppressed() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(3);
        chunks.push(Ok(Bytes::from_static(b"payload")));
        let mut response = response(chunks, &polls, &dropped, 64);

        assert_eq!(next(&mut response).await.unwrap().unwrap(), b"payload"[..]);
        assert_eq!(polls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn direct_byte_stream_never_exposes_empty_items() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_POLL + 1);
        chunks.push(Ok(Bytes::from_static(b"first")));
        chunks.push(Ok(Bytes::new()));
        chunks.push(Ok(Bytes::from_static(b"second")));
        let mut response = response(chunks, &polls, &dropped, 64);
        let mut yielded = Vec::new();

        while let Some(item) = next(&mut response).await {
            let chunk = item.expect("bounded stream item");
            assert!(!chunk.is_empty());
            yielded.push(chunk);
        }

        assert_eq!(
            yielded,
            [Bytes::from_static(b"first"), Bytes::from_static(b"second")]
        );
    }

    #[test]
    fn per_poll_empty_chunk_budget_returns_pending() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_POLL);
        chunks.push(Ok(Bytes::from_static(b"deferred")));
        let mut response = response(chunks, &polls, &dropped, 64);
        let wakes = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&wakes));
        let mut context = Context::from_waker(&waker);

        assert!(matches!(
            Pin::new(&mut response).poll_next(&mut context),
            Poll::Pending
        ));
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_POLL
        );
    }

    #[test]
    fn per_poll_budget_wakes_the_task() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_POLL);
        chunks.push(Ok(Bytes::from_static(b"deferred")));
        let mut response = response(chunks, &polls, &dropped, 64);
        let wakes = Arc::new(WakeCounter::default());
        let waker = Waker::from(Arc::clone(&wakes));
        let mut context = Context::from_waker(&waker);

        assert!(matches!(
            Pin::new(&mut response).poll_next(&mut context),
            Poll::Pending
        ));
        assert_eq!(wakes.wakes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn total_empty_chunk_exact_limit_succeeds() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS);
        chunks.push(Ok(Bytes::from_static(b"exact-limit")));
        let mut response = response(chunks, &polls, &dropped, 64);

        assert_eq!(
            next(&mut response).await.unwrap().unwrap(),
            b"exact-limit"[..]
        );
        assert!(next(&mut response).await.is_none());
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 2);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn total_empty_chunk_first_excess_is_terminal() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = response(
            empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1),
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut response)
            .await
            .unwrap()
            .expect_err("first excess empty chunk");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 1);
        assert!(dropped.load(Ordering::SeqCst));
        assert!(next(&mut response).await.is_none());
    }

    #[tokio::test]
    async fn empty_chunk_limit_error_is_redacted() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = response(
            empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1),
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut response)
            .await
            .unwrap()
            .expect_err("empty-chunk hard limit");
        let rendered = format!("{error} {error:?}");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        assert!(error.http_response().is_none());
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
        assert!(!rendered.contains("empty chunk"));
        assert!(!rendered.contains("http://"));
        assert!(!rendered.contains("authorization"));
    }

    #[tokio::test]
    async fn upstream_error_after_empty_chunks_is_preserved_safely() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let upstream = ApiError::classified(
            EndpointId::from_static("stream.upstream"),
            http::Method::GET,
            ErrorCategory::Connect,
        );
        let mut response = response(
            [Ok(Bytes::new()), Ok(Bytes::new()), Err(upstream)],
            &polls,
            &dropped,
            64,
        );

        let error = next(&mut response)
            .await
            .unwrap()
            .expect_err("safe upstream error");
        assert_eq!(error.category(), ErrorCategory::Connect);
        assert_eq!(error.endpoint(), "stream.upstream");
        assert_eq!(polls.load(Ordering::SeqCst), 3);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn eof_after_empty_chunks_completes_normally() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut response = response(empty_chunks(5), &polls, &dropped, 64);

        assert!(next(&mut response).await.is_none());
        assert_eq!(polls.load(Ordering::SeqCst), 6);
        assert!(dropped.load(Ordering::SeqCst));
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
    async fn nonempty_chunk_byte_limit_is_unchanged() {
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
