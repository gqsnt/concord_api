//! Read-only lifecycle observation installed on one immutable client snapshot.
//!
//! Required features: none. Mode: construction-only. Documentation owners:
//! `docs/execution_policy.md` and `docs/extensions.md`. Proving test:
//! `tests/construction.rs`.

// ANCHOR: hooks_extension
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::api;
use concord::observation::{DebugLevel, OperationEvent, OperationObserver};

api! {
    client ObservedApi {
        timeout 30s;
        base "http://observed.example.test/";


    GET health() -> () via Empty;
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

impl OperationObserver for CountingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(_) => {
                self.attempts.fetch_add(1, Ordering::Relaxed);
            }
            OperationEvent::ResponseHeadObserved(_) => {
                self.responses.fetch_add(1, Ordering::Relaxed);
            }
            OperationEvent::OperationFinished(_) | OperationEvent::PaginationFinished(_) => {
                self.completions.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}
// ANCHOR_END: hooks_implementation

/// Installs shared observation and body-free structural debug events.
// ANCHOR: hooks_client
pub fn construct() -> Result<ObservedApi, concord::ClientBuildError> {
    let observation = Arc::new(CountingHooks::default());
    ObservedApi::builder()
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(observation)
                .debug(DebugLevel::Basic)
        })
        .build()
}
// ANCHOR_END: hooks_client
// ANCHOR_END: hooks_extension
