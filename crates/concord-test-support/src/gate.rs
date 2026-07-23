use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tokio::sync::Notify;

/// A scoped, one-way deterministic synchronization gate.
#[derive(Clone)]
pub struct Gate {
    inner: Arc<GateInner>,
}

struct GateInner {
    open: AtomicBool,
    arrivals: AtomicUsize,
    changed: Notify,
    reached: Notify,
}

impl Gate {
    /// Creates a closed gate.
    #[must_use]
    pub fn closed() -> Self {
        Self {
            inner: Arc::new(GateInner {
                open: AtomicBool::new(false),
                arrivals: AtomicUsize::new(0),
                changed: Notify::new(),
                reached: Notify::new(),
            }),
        }
    }

    /// Opens the gate permanently and wakes current waiters.
    pub fn open(&self) {
        if !self.inner.open.swap(true, Ordering::AcqRel) {
            self.inner.changed.notify_waiters();
        }
    }

    /// Returns whether the gate has opened.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.inner.open.load(Ordering::Acquire)
    }

    /// Returns how many waits have reached this gate.
    #[must_use]
    pub fn arrivals(&self) -> usize {
        self.inner.arrivals.load(Ordering::Acquire)
    }

    /// Waits until at least one execution has reached the gate.
    pub async fn wait_until_reached(&self) {
        self.wait_until_arrivals(1).await;
    }

    /// Waits until at least `expected` executions have reached the gate.
    pub async fn wait_until_arrivals(&self, expected: usize) {
        loop {
            if self.arrivals() >= expected {
                return;
            }
            let reached = self.inner.reached.notified();
            if self.arrivals() >= expected {
                return;
            }
            reached.await;
        }
    }

    /// Waits until the gate opens.
    pub async fn wait(&self) {
        self.inner.arrivals.fetch_add(1, Ordering::AcqRel);
        self.inner.reached.notify_waiters();
        loop {
            if self.is_open() {
                return;
            }
            let changed = self.inner.changed.notified();
            if self.is_open() {
                return;
            }
            changed.await;
        }
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::closed()
    }
}

impl fmt::Debug for Gate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Gate")
            .field("open", &self.is_open())
            .field("arrivals", &self.arrivals())
            .finish()
    }
}
