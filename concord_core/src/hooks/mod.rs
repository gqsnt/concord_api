//! Safe request lifecycle observation and built-in debug selection.

use std::future::Future;
use std::pin::Pin;

mod debug;
mod events;

pub(crate) use debug::DebugDispatcher;
pub use events::{
    BeforeSend, BodyKind, CompletionEvent, CompletionOutcome, PaginationCompletionEvent,
    ResponseHeadEvent,
};

use crate::error::HookError;

/// A boxed, sendable future used by stable asynchronous extension contracts.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Built-in body-free debug presentation volume.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DebugLevel {
    #[default]
    None,
    Basic,
    Headers,
}

/// Safe lifecycle callbacks shared by every clone of one built client.
///
/// Calls may overlap. Result-determining hooks run under the operation
/// deadline. Completion hooks observe an already-fixed result outside that
/// deadline and must bound their own work.
pub trait RuntimeHooks: Send + Sync + 'static {
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(&'a self, _context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

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

        fn on_response_head<'a>(&'a self, _context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                self.response.fetch_add(1, Ordering::SeqCst);
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
            recovery_index: 0,
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
                recovery_index: 0,
                status: StatusCode::NO_CONTENT,
                headers: SanitizedHeaders::new(&headers),
            })
            .await;
        hooks
            .on_complete(CompletionEvent {
                operation_id,
                parent_operation_id: None,
                endpoint: "HooksApi::health::ping",
                method: &method,
                safe_url: "https://hooks.example.test/health",
                page_index: 0,
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
