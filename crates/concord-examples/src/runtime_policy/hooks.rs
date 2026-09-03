//! Safe lifecycle hooks installed on one immutable client snapshot.
//!
//! Required features: none. Mode: construction-only. Documentation owners:
//! `docs/execution_policy.md` and `docs/extensions.md`. Proving test:
//! `tests/construction.rs`.

// ANCHOR: hooks_extension
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::api;
use concord::error::GateError;
use concord::observation::{
    BeforeSend, BoxFuture, CompletionEvent, CompletionObserver, DebugLevel, RequestGate,
    ResponseGate, ResponseHeadEvent,
};

api! {
    client ObservedApi {
        timeout off;
        base "http://observed.example.test/";


    GET health() -> () using NoContent;
}}

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

impl RequestGate for CountingHooks {
    fn before_send<'a>(&'a self, _event: BeforeSend<'a>) -> BoxFuture<'a, Result<(), GateError>> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }
}

impl ResponseGate for CountingHooks {
    fn before_response_handling<'a>(
        &'a self,
        _event: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), GateError>> {
        Box::pin(async move {
            self.responses.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }
}

impl CompletionObserver for CountingHooks {
    fn on_complete(&self, _event: CompletionEvent<'_>) {
        self.completions.fetch_add(1, Ordering::Relaxed);
    }
}
// ANCHOR_END: hooks_implementation

/// Installs shared hooks and body-free structural debug events.
// ANCHOR: hooks_client
pub fn construct() -> Result<ObservedApi, concord::ClientBuildError> {
    let observation = Arc::new(CountingHooks::default());
    ObservedApi::builder()
        .configure_runtime(move |runtime| {
            runtime
                .request_gate(observation.clone())
                .response_gate(observation.clone())
                .completion_observer(observation)
                .debug(DebugLevel::Basic)
        })
        .build()
}
// ANCHOR_END: hooks_client
// ANCHOR_END: hooks_extension
