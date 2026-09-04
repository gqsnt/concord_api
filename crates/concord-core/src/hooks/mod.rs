//! Read-only, bounded, sanitized operational observation.

use std::future::Future;
use std::pin::Pin;

mod debug;
mod events;

pub use crate::body::BodyKind;
pub(crate) use debug::DebugDispatcher;
pub use events::{
    AttemptInfo, AttemptStarted, CompletionOutcome, OperationEvent, OperationFinished,
    PaginationFinished, ResponseHeadObserved, StreamFinished, StreamKind, StreamOutcome,
};

/// A boxed, sendable future used by asynchronous semantic extension contracts.
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

/// Synchronous, read-only observation of managed lifecycle evidence.
///
/// An observer receives bounded credential-free/redacted events, returns no
/// decision, and cannot directly transform the caller's result. Calls are
/// synchronous, may overlap across threads, and must be prompt and bounded.
/// Applications that export asynchronously must close over a separately owned
/// bounded queue/sink with an explicit backpressure and shutdown policy.
pub trait OperationObserver: Send + Sync + 'static {
    /// Observes one sanitized event. Panics are contained by the core runtime.
    fn observe(&self, _event: OperationEvent<'_>) {}
}

pub(crate) struct NoopOperationObserver;
impl OperationObserver for NoopOperationObserver {}

pub(crate) fn default_operation_observer() -> std::sync::Arc<dyn OperationObserver> {
    std::sync::Arc::new(NoopOperationObserver)
}

pub(crate) fn observe(observer: &dyn OperationObserver, event: OperationEvent<'_>) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer.observe(event)));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use http::{HeaderMap, Method, StatusCode};

    use super::*;
    use crate::operation::OperationId;
    use crate::redaction::SanitizedHeaders;

    struct CountingObserver(AtomicUsize);

    impl OperationObserver for CountingObserver {
        fn observe(&self, _event: OperationEvent<'_>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn id() -> OperationId {
        OperationId::allocate().expect("operation ID")
    }

    fn attempt<'a>(
        operation_id: OperationId,
        method: &'a Method,
        headers: &'a HeaderMap,
    ) -> AttemptStarted<'a> {
        AttemptStarted {
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

    #[test]
    fn observation_is_synchronous_and_non_authoritative() {
        let method = Method::GET;
        let headers = HeaderMap::new();
        let observer = CountingObserver(AtomicUsize::new(0));
        observer.observe(OperationEvent::AttemptStarted(attempt(
            id(),
            &method,
            &headers,
        )));
        observer.observe(OperationEvent::ResponseHeadObserved(ResponseHeadObserved {
            operation_id: id(),
            parent_operation_id: None,
            endpoint: "HooksApi::health::ping",
            method: &method,
            observed_url: "https://hooks.example.test/health",
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            status: StatusCode::NO_CONTENT,
            headers: SanitizedHeaders::new(&headers),
        }));
        observer.observe(OperationEvent::OperationFinished(OperationFinished {
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
        }));
        assert_eq!(observer.0.load(Ordering::SeqCst), 3);
    }
}
