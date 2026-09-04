//! Read-only, bounded, sanitized operational observation.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

mod debug;
mod events;

pub use crate::body::BodyKind;
pub(crate) use debug::DebugDispatcher;
pub use events::{
    AttemptInfo, AttemptStarted, CompletionOutcome, OperationEvent, OperationFinished,
    PaginationFinished, RequestTarget, ResponseHeadObserved, StreamFinished, StreamKind,
    StreamOutcome,
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
    /// Also emits value-free request-target counts and typed response facts.
    ///
    /// Request and response header names and values are never included.
    Headers,
}

/// Synchronous, read-only observation of managed lifecycle evidence.
///
/// An observer receives bounded, value-free request events, returns no decision,
/// and cannot directly transform the caller's result. Exact URLs and header
/// names or values are absent. Calls are
/// synchronous, may overlap across threads, and must be prompt, finite,
/// nonblocking, and non-panicking. Core contains a panic but cannot preempt a
/// synchronous callback that blocks or runs forever. Time spent in an
/// intermediate callback reduces the operation's remaining budget; core checks
/// that budget after the callback before starting more remote work. A final
/// callback runs after result fixation and may delay delivery beyond the
/// nominal deadline without changing that result.
/// Applications that export asynchronously must close over a separately owned
/// bounded queue/sink with an explicit backpressure and shutdown policy.
pub trait OperationObserver: Send + Sync + 'static {
    /// Observes one sanitized event. Panics are contained by the core runtime
    /// and counted only in a private bounded diagnostic; observer health is not
    /// publicly observable in v2.1. Core never creates a detached task for this
    /// callback.
    fn observe(&self, _event: OperationEvent<'_>) {}
}

pub(crate) struct NoopOperationObserver;
impl OperationObserver for NoopOperationObserver {}

pub(crate) fn default_operation_observer() -> std::sync::Arc<dyn OperationObserver> {
    std::sync::Arc::new(NoopOperationObserver)
}

static OBSERVER_PANIC_COUNT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn observe(observer: &dyn OperationObserver, event: OperationEvent<'_>) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer.observe(event))).is_err() {
        let _ = OBSERVER_PANIC_COUNT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            Some(count.saturating_add(1))
        });
    }
}

#[cfg(test)]
pub(crate) fn observer_panic_count() -> u64 {
    OBSERVER_PANIC_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use http::{Method, StatusCode};

    use super::*;
    use crate::operation::OperationId;

    struct CountingObserver(AtomicUsize);

    impl OperationObserver for CountingObserver {
        fn observe(&self, _event: OperationEvent<'_>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct PanickingObserver;

    impl OperationObserver for PanickingObserver {
        fn observe(&self, _event: OperationEvent<'_>) {
            panic!("observer panic test sentinel");
        }
    }

    fn id() -> OperationId {
        OperationId::allocate().expect("operation ID")
    }

    fn attempt<'a>(
        operation_id: OperationId,
        method: &'a Method,
        target: RequestTarget,
    ) -> AttemptStarted<'a> {
        AttemptStarted {
            operation_id,
            parent_operation_id: None,
            endpoint: "HooksApi::health::ping",
            method,
            target,
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            body_kind: BodyKind::Empty,
            known_body_length: Some(0),
        }
    }

    #[test]
    fn observation_is_synchronous_and_non_authoritative() {
        let method = Method::GET;
        let target = RequestTarget::from_url(
            &url::Url::parse("https://hooks.example.test/health").expect("hook target"),
            0,
        );
        let observer = CountingObserver(AtomicUsize::new(0));
        observer.observe(OperationEvent::AttemptStarted(attempt(
            id(),
            &method,
            target,
        )));
        observer.observe(OperationEvent::ResponseHeadObserved(ResponseHeadObserved {
            operation_id: id(),
            parent_operation_id: None,
            endpoint: "HooksApi::health::ping",
            method: &method,
            target,
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            status: StatusCode::NO_CONTENT,
            content_length: Some(0),
            server_delay: None,
        }));
        observer.observe(OperationEvent::OperationFinished(OperationFinished {
            operation_id: id(),
            parent_operation_id: None,
            endpoint: "HooksApi::health::ping",
            method: &method,
            target,
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

    #[test]
    fn contained_observer_panics_increment_the_bounded_private_diagnostic() {
        let method = Method::GET;
        let target = RequestTarget::from_url(
            &url::Url::parse("https://hooks.example.test/health").expect("hook target"),
            0,
        );
        let before = observer_panic_count();
        observe(
            &PanickingObserver,
            OperationEvent::AttemptStarted(attempt(id(), &method, target)),
        );
        assert!(observer_panic_count() >= before.saturating_add(1));
    }
}
