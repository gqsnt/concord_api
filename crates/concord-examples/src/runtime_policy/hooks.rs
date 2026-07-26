//! Safe lifecycle hooks installed on one immutable client snapshot.
//!
//! Required features: none. Mode: construction-only. Documentation owner:
//! `docs/features/hooks-and-observability.md`. Proving test:
//! `tests/construction.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::api;
use concord::error::HookError;
use concord::hooks::{
    BeforeSend, BoxFuture, CompletionEvent, DebugLevel, ResponseHeadEvent, RuntimeHooks,
};

api! {
    client ObservedApi {
        base "http://observed.example.test/";
    }

    GET health() -> NoContent;
}

/// A bounded counter-only observer suitable for one client snapshot.
// ANCHOR: hooks_implementation
#[derive(Default)]
pub struct CountingHooks {
    attempts: AtomicUsize,
    responses: AtomicUsize,
    completions: AtomicUsize,
}

impl CountingHooks {
    /// Returns the number of before-send callbacks observed so far.
    #[must_use]
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Relaxed)
    }
}

impl RuntimeHooks for CountingHooks {
    fn before_send<'a>(&'a self, _event: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }

    fn on_response_head<'a>(
        &'a self,
        _event: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async move {
            self.responses.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }

    fn on_complete<'a>(&'a self, _event: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.completions.fetch_add(1, Ordering::Relaxed);
        })
    }
}
// ANCHOR_END: hooks_implementation

/// Installs shared hooks and body-free structural debug events.
// ANCHOR: hooks_client
pub fn construct() -> Result<ObservedApi, concord::ClientBuildError> {
    ObservedApi::builder()
        .configure_runtime(|runtime| {
            runtime
                .hooks(Arc::new(CountingHooks::default()))
                .debug(DebugLevel::Basic)
        })
        .build()
}
// ANCHOR_END: hooks_client
