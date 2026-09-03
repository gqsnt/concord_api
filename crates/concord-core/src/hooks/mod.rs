//! Credential-free decision gates and fixed-outcome completion observation.

use std::future::Future;
use std::pin::Pin;

mod debug;
mod events;

pub use crate::body::BodyKind;
pub(crate) use debug::DebugDispatcher;
pub use events::{
    AttemptInfo, BeforeSend, CompletionEvent, CompletionOutcome, PaginationCompletionEvent,
    ResponseHeadEvent,
};

use crate::error::GateError;

/// A boxed, sendable future used by stable asynchronous extension contracts.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Built-in body-free debug presentation volume.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DebugLevel {
    /// Disables built-in request and response debug events.
    #[default]
    None,
    /// Emits structural request and response metadata without headers.
    Basic,
    /// Also emits sanitized request and response headers.
    Headers,
}

/// Pre-dispatch decision authority for one managed attempt.
///
/// The gate receives only bounded credential-free/redacted metadata. It cannot
/// inspect body values or bytes, credentials, rate keys, or native requests,
/// and cannot mutate the request. Its future runs under the aggregate
/// operation deadline and is dropped when the caller cancels the request.
pub trait RequestGate: Send + Sync + 'static {
    /// Approves or rejects one attempt before body materialization and native
    /// dispatch begin.
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), GateError>> {
        Box::pin(async { Ok(()) })
    }
}

/// Response-head decision authority before challenge/status/body handling.
///
/// This gate sees bounded sanitized response metadata, never a native response
/// or response body. Its future runs under the aggregate operation deadline.
pub trait ResponseGate: Send + Sync + 'static {
    /// Approves or rejects handling of one received response head.
    fn before_response_handling<'a>(
        &'a self,
        _context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), GateError>> {
        Box::pin(async { Ok(()) })
    }
}

/// Synchronous non-authoritative observation of already-fixed outcomes.
///
/// An observer receives bounded credential-free/redacted events, returns no
/// decision, and cannot transform the caller's result. Calls are synchronous,
/// may overlap across threads, and must be trusted, nonblocking, and bounded.
/// Applications that export asynchronously must close over a separately owned
/// bounded queue/sink with an explicit backpressure and shutdown policy.
pub trait CompletionObserver: Send + Sync + 'static {
    /// Records one already-fixed request or page outcome.
    fn on_complete(&self, _event: CompletionEvent<'_>) {}

    /// Records one already-fixed aggregate pagination outcome.
    fn on_pagination_complete(&self, _event: PaginationCompletionEvent<'_>) {}
}

pub(crate) struct NoopRequestGate;
impl RequestGate for NoopRequestGate {}

pub(crate) struct NoopResponseGate;
impl ResponseGate for NoopResponseGate {}

pub(crate) struct NoopCompletionObserver;
impl CompletionObserver for NoopCompletionObserver {}

pub(crate) fn default_request_gate() -> std::sync::Arc<dyn RequestGate> {
    std::sync::Arc::new(NoopRequestGate)
}

pub(crate) fn default_response_gate() -> std::sync::Arc<dyn ResponseGate> {
    std::sync::Arc::new(NoopResponseGate)
}

pub(crate) fn default_completion_observer() -> std::sync::Arc<dyn CompletionObserver> {
    std::sync::Arc::new(NoopCompletionObserver)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use http::{HeaderMap, Method, StatusCode};

    use super::*;
    use crate::operation::OperationId;
    use crate::redaction::SanitizedHeaders;

    struct CountingPolicy {
        before: AtomicUsize,
        response: AtomicUsize,
        complete: AtomicUsize,
        pagination: AtomicUsize,
    }

    impl CountingPolicy {
        const fn new() -> Self {
            Self {
                before: AtomicUsize::new(0),
                response: AtomicUsize::new(0),
                complete: AtomicUsize::new(0),
                pagination: AtomicUsize::new(0),
            }
        }
    }

    impl RequestGate for CountingPolicy {
        fn before_send<'a>(
            &'a self,
            _context: BeforeSend<'a>,
        ) -> BoxFuture<'a, Result<(), GateError>> {
            Box::pin(async move {
                self.before.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    impl ResponseGate for CountingPolicy {
        fn before_response_handling<'a>(
            &'a self,
            _context: ResponseHeadEvent<'a>,
        ) -> BoxFuture<'a, Result<(), GateError>> {
            Box::pin(async move {
                self.response.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    impl CompletionObserver for CountingPolicy {
        fn on_complete(&self, _event: CompletionEvent<'_>) {
            self.complete.fetch_add(1, Ordering::SeqCst);
        }

        fn on_pagination_complete(&self, _event: PaginationCompletionEvent<'_>) {
            self.pagination.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn id() -> OperationId {
        OperationId::allocate().expect("operation ID")
    }

    fn before<'a>(
        operation_id: OperationId,
        method: &'a Method,
        headers: &'a HeaderMap,
    ) -> BeforeSend<'a> {
        BeforeSend {
            operation_id,
            parent_operation_id: None,
            endpoint: "HooksApi::health::ping",
            method,
            observed_url: "https://hooks.example.test/health",
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            headers: SanitizedHeaders::new(headers),
            body_kind: BodyKind::Empty,
            known_body_length: Some(0),
        }
    }

    #[tokio::test]
    async fn gates_are_async_decisions_and_completion_is_synchronous_observation() {
        let method = Method::GET;
        let headers = HeaderMap::new();
        let policy = CountingPolicy::new();
        policy
            .before_send(before(id(), &method, &headers))
            .await
            .expect("request gate");
        policy
            .before_response_handling(ResponseHeadEvent {
                operation_id: id(),
                parent_operation_id: None,
                endpoint: "HooksApi::health::ping",
                method: &method,
                observed_url: "https://hooks.example.test/health",
                page_index: 0,
                attempt: AttemptInfo::INITIAL,
                status: StatusCode::NO_CONTENT,
                headers: SanitizedHeaders::new(&headers),
            })
            .await
            .expect("response gate");
        policy.on_complete(CompletionEvent {
            operation_id: id(),
            parent_operation_id: None,
            endpoint: "HooksApi::health::ping",
            method: &method,
            observed_url: "https://hooks.example.test/health",
            page_index: 0,
            attempts_started: 1,
            policy_retries: 0,
            recovered_authentication: false,
            elapsed: std::time::Duration::ZERO,
            final_status: Some(StatusCode::NO_CONTENT),
            outcome: CompletionOutcome::Success,
        });
        assert_eq!(policy.before.load(Ordering::SeqCst), 1);
        assert_eq!(policy.response.load(Ordering::SeqCst), 1);
        assert_eq!(policy.complete.load(Ordering::SeqCst), 1);
    }
}
