//! Safe request lifecycle observation and built-in debug selection.

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

use crate::error::HookError;

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

/// Safe lifecycle callbacks shared by every clone of one built client.
///
/// Calls may overlap. Result-determining hooks run under the operation
/// deadline. Completion hooks observe an already-fixed result outside that
/// deadline. Completion is awaited exactly once and blocks delivery of that
/// fixed result, so completion observers are a trusted application liveness
/// boundary and must bound their own work. Hook metadata is credential-sanitized
/// but may still contain operational identifiers or personally identifiable
/// information; implementations own all logging and storage. They must bound
/// CPU, memory, retries, maps, queues, spawned tasks, metric cardinality, and
/// external I/O. Hook futures are dropped on cancellation, without rollback of
/// completed extension effects. Concord does not automatically time-limit
/// completion callbacks or bound custom hook state, synchronous work before a
/// future is returned, or application-created tasks.
pub trait RuntimeHooks: Send + Sync + 'static {
    /// Approves or rejects one attempt before body materialization and native
    /// execution begin.
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Ok(()) })
    }

    /// Observes or rejects one final response head after core limiter
    /// observation and before challenge, status, or body handling.
    fn on_response_head<'a>(
        &'a self,
        _context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Ok(()) })
    }

    /// Observes one already-fixed direct request or page result outside its
    /// deadline and delays delivery until this future completes.
    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    /// Observes one already-fixed aggregate pagination result outside its
    /// deadline and delays delivery until this future completes.
    fn on_pagination_complete<'a>(
        &'a self,
        _context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

pub(crate) struct NoopHooks;

impl RuntimeHooks for NoopHooks {}

pub(crate) fn default_hooks() -> std::sync::Arc<dyn RuntimeHooks> {
    std::sync::Arc::new(NoopHooks)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use http::{HeaderMap, Method, StatusCode};

    use super::*;
    use crate::operation::OperationId;
    use crate::redaction::SanitizedHeaders;

    struct CountingHooks {
        before: AtomicUsize,
        response: AtomicUsize,
        complete: AtomicUsize,
        pagination: AtomicUsize,
    }

    impl CountingHooks {
        const fn new() -> Self {
            Self {
                before: AtomicUsize::new(0),
                response: AtomicUsize::new(0),
                complete: AtomicUsize::new(0),
                pagination: AtomicUsize::new(0),
            }
        }
    }

    impl RuntimeHooks for CountingHooks {
        fn before_send<'a>(
            &'a self,
            _context: BeforeSend<'a>,
        ) -> BoxFuture<'a, Result<(), HookError>> {
            Box::pin(async move {
                self.before.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }

        fn on_response_head<'a>(
            &'a self,
            _context: ResponseHeadEvent<'a>,
        ) -> BoxFuture<'a, Result<(), HookError>> {
            Box::pin(async move {
                self.response.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }

        fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                self.complete.fetch_add(1, Ordering::SeqCst);
            })
        }

        fn on_pagination_complete<'a>(
            &'a self,
            _context: PaginationCompletionEvent<'a>,
        ) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                self.pagination.fetch_add(1, Ordering::SeqCst);
            })
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
            safe_url: "https://hooks.example.test/health",
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            headers: SanitizedHeaders::new(headers),
            body_kind: BodyKind::Empty,
            known_body_length: Some(0),
        }
    }

    #[tokio::test]
    async fn default_hooks_are_noop_and_concurrent_calls_overlap_safely() {
        let method = Method::GET;
        let headers = HeaderMap::new();
        let noop = NoopHooks;
        noop.before_send(before(id(), &method, &headers))
            .await
            .expect("default before-send");

        let hooks = CountingHooks::new();
        let first = hooks.before_send(before(id(), &method, &headers));
        let second = hooks.before_send(before(id(), &method, &headers));
        let (first, second) = tokio::join!(first, second);
        first.expect("first hook");
        second.expect("second hook");
        assert_eq!(hooks.before.load(Ordering::SeqCst), 2);

        let operation_id = id();
        hooks
            .on_response_head(ResponseHeadEvent {
                operation_id,
                parent_operation_id: None,
                endpoint: "HooksApi::health::ping",
                method: &method,
                safe_url: "https://hooks.example.test/health",
                page_index: 0,
                attempt: AttemptInfo::INITIAL,
                status: StatusCode::NO_CONTENT,
                headers: SanitizedHeaders::new(&headers),
            })
            .await
            .expect("response hook");
        hooks
            .on_complete(CompletionEvent {
                operation_id,
                parent_operation_id: None,
                endpoint: "HooksApi::health::ping",
                method: &method,
                safe_url: "https://hooks.example.test/health",
                page_index: 0,
                attempts_started: 1,
                policy_retries: 0,
                recovered_authentication: false,
                elapsed: std::time::Duration::ZERO,
                final_status: Some(StatusCode::NO_CONTENT),
                outcome: CompletionOutcome::Success,
            })
            .await;
        hooks
            .on_pagination_complete(PaginationCompletionEvent {
                operation_id: id(),
                endpoint: "HooksApi::health::ping",
                method: &method,
                pages_completed: 0,
                items_collected: 0,
                attempts_started: 0,
                policy_retries: 0,
                recovered_authentication: false,
                elapsed: std::time::Duration::ZERO,
                final_status: None,
                outcome: CompletionOutcome::Success,
            })
            .await;

        assert_eq!(hooks.response.load(Ordering::SeqCst), 1);
        assert_eq!(hooks.complete.load(Ordering::SeqCst), 1);
        assert_eq!(hooks.pagination.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pagination_completion_debug_has_no_continuation_or_private_state() {
        let method = Method::GET;
        let event = PaginationCompletionEvent {
            operation_id: id(),
            endpoint: "HooksApi::items::list",
            method: &method,
            pages_completed: 3,
            items_collected: 7,
            attempts_started: 3,
            policy_retries: 0,
            recovered_authentication: false,
            elapsed: std::time::Duration::from_millis(4),
            final_status: Some(StatusCode::OK),
            outcome: CompletionOutcome::Success,
        };
        let debug = format!("{event:?}");
        for forbidden in [
            "cursor-sentinel",
            "continuation-sentinel",
            "identity-sentinel",
            "partial-item-sentinel",
        ] {
            assert!(!debug.contains(forbidden));
        }
    }
}
