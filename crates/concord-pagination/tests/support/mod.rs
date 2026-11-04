//! Deterministic controllable futures for the traversal tests.
//!
//! A [`Server`] stands in for a provider: its `call` is the request factory. Every call is
//! recorded, returns a future that completes only when the test `open`s that key (or never), and
//! counts as active from the factory call until the future completes or is dropped. Nothing here
//! uses a timer or a runtime: [`now`] polls once with a no-op waker, so a step either completes or
//! is observably pending. The real-runtime tests drive the same futures with real wakers.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Polls `future` once: `Some(output)` if it completed, `None` if it is pending (it is then
/// dropped, which also exercises cancellation safety of `next`).
pub fn now<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}

enum Slot<T> {
    Waiting(Option<Waker>),
    Open(T),
    Taken,
}

struct Lever<K, T> {
    key: K,
    slot: Rc<RefCell<Slot<T>>>,
}

struct Inner<K, T> {
    calls: RefCell<Vec<K>>,
    levers: RefCell<Vec<Lever<K, T>>>,
    active: Counter,
}

/// A fake provider whose responses the test releases key by key.
pub struct Server<K, T> {
    inner: Rc<Inner<K, T>>,
}

impl<K, T> Clone for Server<K, T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<K: Clone + PartialEq + std::fmt::Debug, T> Default for Server<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Clone + PartialEq + std::fmt::Debug, T> Server<K, T> {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                calls: RefCell::new(Vec::new()),
                levers: RefCell::new(Vec::new()),
                active: Counter::new(),
            }),
        }
    }

    /// The request factory: records the call and returns a future pending until `open(key)`.
    pub fn call(&self, key: K) -> Response<T> {
        self.inner.calls.borrow_mut().push(key.clone());
        let slot = Rc::new(RefCell::new(Slot::Waiting(None)));
        self.inner.levers.borrow_mut().push(Lever {
            key,
            slot: Rc::clone(&slot),
        });
        Response {
            slot,
            _active: self.inner.active.enter(),
        }
    }

    /// Completes the most recent call for `key` with `value` and wakes its poller.
    pub fn open(&self, key: &K, value: T) {
        let levers = self.inner.levers.borrow();
        let lever = levers
            .iter()
            .rev()
            .find(|lever| &lever.key == key && matches!(*lever.slot.borrow(), Slot::Waiting(_)))
            .unwrap_or_else(|| panic!("no pending call for {key:?}"));
        let previous = std::mem::replace(&mut *lever.slot.borrow_mut(), Slot::Open(value));
        if let Slot::Waiting(Some(waker)) = previous {
            waker.wake();
        }
    }

    /// Whether the future of the most recent call for `key` was dropped before completing.
    pub fn dropped(&self, key: &K) -> bool {
        let levers = self.inner.levers.borrow();
        let lever = levers
            .iter()
            .rev()
            .find(|lever| &lever.key == key)
            .unwrap_or_else(|| panic!("no call for {key:?}"));
        Rc::strong_count(&lever.slot) == 1 && matches!(*lever.slot.borrow(), Slot::Waiting(_))
    }

    /// Factory calls so far, in order.
    pub fn calls(&self) -> Vec<K> {
        self.inner.calls.borrow().clone()
    }

    /// Number of factory calls so far.
    pub fn call_count(&self) -> usize {
        self.inner.calls.borrow().len()
    }

    /// Futures created by `call` and neither completed nor dropped.
    pub fn active(&self) -> usize {
        self.inner.active.live()
    }

    /// The largest `active` value seen.
    pub fn peak(&self) -> usize {
        self.inner.active.peak()
    }
}

/// The future returned by [`Server::call`].
pub struct Response<T> {
    slot: Rc<RefCell<Slot<T>>>,
    _active: Token,
}

impl<T> Future for Response<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut slot = self.slot.borrow_mut();
        match std::mem::replace(&mut *slot, Slot::Taken) {
            Slot::Open(value) => Poll::Ready(value),
            Slot::Waiting(_) => {
                *slot = Slot::Waiting(Some(cx.waker().clone()));
                Poll::Pending
            }
            Slot::Taken => panic!("polled after completion"),
        }
    }
}

/// Counts live values (futures, results) and tracks the peak.
#[derive(Clone, Default)]
pub struct Counter {
    cells: Rc<(Cell<usize>, Cell<usize>)>,
}

impl Counter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A token that counts as live until dropped.
    pub fn enter(&self) -> Token {
        let (live, peak) = &*self.cells;
        live.set(live.get() + 1);
        peak.set(peak.get().max(live.get()));
        Token {
            cells: Rc::clone(&self.cells),
        }
    }

    pub fn live(&self) -> usize {
        self.cells.0.get()
    }

    pub fn peak(&self) -> usize {
        self.cells.1.get()
    }
}

/// See [`Counter::enter`].
pub struct Token {
    cells: Rc<(Cell<usize>, Cell<usize>)>,
}

impl Drop for Token {
    fn drop(&mut self) {
        let live = &self.cells.0;
        live.set(live.get() - 1);
    }
}
