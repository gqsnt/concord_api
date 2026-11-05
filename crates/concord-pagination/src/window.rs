//! The bounded window shared by independent plans and jobs.
//!
//! It owns the admitted futures (polled concurrently on the caller's task through
//! `FuturesUnordered`), the completed results retained for ordered emission, and the identity of
//! every admitted key that has not been emitted successfully. Its invariant:
//! `in flight + retained <= cap`. Admission and the budget are the caller's (`Independent`,
//! `Jobs`); the window only says whether it has room.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::{FuturesUnordered, Stream};

use crate::types::{Checkpoint, Keyed, OnError, Options, Order, Progress, Unfinished};

/// What `next` returns for one key.
pub(crate) type Emitted<K, R, E> = Result<Keyed<K, R>, Keyed<K, E>>;

/// A future tagged with its admission index. Boxing keeps it `Unpin`, so no pin projection (and no
/// `unsafe`) is needed; one allocation per page or job.
struct Tagged<Fut> {
    index: u64,
    future: Pin<Box<Fut>>,
}

impl<Fut: Future> Future for Tagged<Fut> {
    type Output = (u64, Fut::Output);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let index = self.index;
        self.future.as_mut().poll(cx).map(|output| (index, output))
    }
}

/// Why the window stopped admitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Halt {
    /// `OnError::Stop` observed a failure.
    Failure,
    /// `OnError::Continue` reached the failure limit.
    FailureLimit,
    /// The caller finished early, or the owner stopped it (discovery failure in stop mode).
    Stopped,
}

/// One scheduling step.
pub(crate) enum Step<K, R, E> {
    /// An outcome to return now.
    Emit(Emitted<K, R, E>),
    /// A result was retained; poll again.
    Again,
    /// Nothing is in flight.
    Idle,
    /// Everything in flight is pending; the caller's waker is registered.
    Pending,
}

/// What a finished window hands back.
pub(crate) struct Settled<K, R, E> {
    pub(crate) unfinished: Vec<Keyed<K, Unfinished>>,
    pub(crate) retained: Vec<Keyed<K, R>>,
    pub(crate) unemitted_errors: Vec<Keyed<K, E>>,
}

pub(crate) struct Window<K, R, E, Fut> {
    cap: usize,
    order: Order,
    on_error: OnError,
    failure_limit: Option<u64>,
    active: FuturesUnordered<Tagged<Fut>>,
    /// Admitted keys neither emitted nor failed nor cancelled: in flight or retained.
    open: BTreeMap<u64, K>,
    /// Completed outcomes waiting for ordered emission.
    ready: BTreeMap<u64, Result<R, E>>,
    /// Every failed key (library-owned failure history; bounded by the budget or the failure
    /// limit).
    failed: BTreeMap<u64, K>,
    /// Keys whose futures were dropped by a halt.
    cancelled: BTreeMap<u64, K>,
    next_index: u64,
    next_emit: u64,
    halted: Option<Halt>,
    emitted: u64,
    succeeded: u64,
    failures: u64,
}

impl<K: Clone, R, E, Fut: Future<Output = Result<R, E>>> Window<K, R, E, Fut> {
    pub(crate) fn new(options: &Options) -> Self {
        Self {
            cap: options.window.get(),
            order: options.order,
            on_error: options.on_error,
            failure_limit: options.failure_limit.map(|limit| limit.get() as u64),
            active: FuturesUnordered::new(),
            open: BTreeMap::new(),
            ready: BTreeMap::new(),
            failed: BTreeMap::new(),
            cancelled: BTreeMap::new(),
            next_index: 0,
            next_emit: 0,
            halted: None,
            emitted: 0,
            succeeded: 0,
            failures: 0,
        }
    }

    /// Keys admitted so far (what the budget has been charged).
    pub(crate) fn admitted(&self) -> u64 {
        self.next_index
    }

    pub(crate) fn halted(&self) -> Option<Halt> {
        self.halted
    }

    pub(crate) fn failures(&self) -> u64 {
        self.failures
    }

    /// Whether one more future may be admitted.
    pub(crate) fn has_room(&self) -> bool {
        self.halted.is_none() && self.active.len() + self.ready.len() < self.cap
    }

    /// Nothing runs any more: no future in flight, and either nothing retained or a halt that
    /// will never emit what is retained.
    pub(crate) fn is_settled(&self) -> bool {
        self.active.is_empty() && (self.halted.is_some() || self.ready.is_empty())
    }

    /// Takes ownership of an admitted key's future. The caller has checked `has_room` and charged
    /// its budget.
    pub(crate) fn admit(&mut self, key: K, future: Fut) {
        let index = self.next_index;
        self.next_index += 1;
        self.open.insert(index, key);
        self.active.push(Tagged {
            index,
            future: Box::pin(future),
        });
    }

    /// The next ordered outcome, if it is already retained.
    pub(crate) fn take_ready(&mut self) -> Option<Emitted<K, R, E>> {
        if self.halted.is_some() || self.order != Order::Ordered {
            return None;
        }
        let index = self.next_emit;
        let result = self.ready.remove(&index)?;
        let key = self.open.remove(&index)?;
        self.next_emit += 1;
        Some(self.emit(index, key, result))
    }

    /// Polls the futures in flight once for a completion.
    pub(crate) fn poll_active(&mut self, cx: &mut Context<'_>) -> Step<K, R, E> {
        if self.active.is_empty() {
            return Step::Idle;
        }
        match Pin::new(&mut self.active).poll_next(cx) {
            Poll::Pending => Step::Pending,
            Poll::Ready(None) => Step::Idle,
            Poll::Ready(Some((index, result))) => self.complete(index, result),
        }
    }

    fn complete(&mut self, index: u64, result: Result<R, E>) -> Step<K, R, E> {
        let error = match result {
            Ok(value) => return self.deliver(index, Ok(value)),
            Err(error) => error,
        };
        self.failures += 1;
        if let Some(key) = self.open.get(&index) {
            self.failed.insert(index, key.clone());
        }
        let halt = match self.on_error {
            OnError::Stop => Some(Halt::Failure),
            OnError::Continue => self
                .failure_limit
                .filter(|limit| self.failures >= *limit)
                .map(|_| Halt::FailureLimit),
        };
        match halt {
            // The observed failure is returned now, ahead of any earlier stalled key; every
            // other future is dropped first.
            Some(halt) => match self.open.remove(&index) {
                Some(key) => {
                    if index == self.next_emit {
                        self.next_emit += 1;
                    }
                    self.halt(halt);
                    Step::Emit(self.emit(index, key, Err(error)))
                }
                None => {
                    self.halt(halt);
                    Step::Again
                }
            },
            None => self.deliver(index, Err(error)),
        }
    }

    /// Emits a completion now, or retains it for ordered emission.
    fn deliver(&mut self, index: u64, result: Result<R, E>) -> Step<K, R, E> {
        if self.order == Order::Ordered && index != self.next_emit {
            self.ready.insert(index, result);
            return Step::Again;
        }
        let Some(key) = self.open.remove(&index) else {
            return Step::Again;
        };
        if index == self.next_emit {
            self.next_emit += 1;
        }
        Step::Emit(self.emit(index, key, result))
    }

    fn emit(&mut self, index: u64, key: K, result: Result<R, E>) -> Emitted<K, R, E> {
        self.emitted += 1;
        match result {
            Ok(value) => {
                self.succeeded += 1;
                Ok(Keyed { index, key, value })
            }
            Err(value) => Err(Keyed { index, key, value }),
        }
    }

    /// Turns an emitted success into a failure (a first page whose plan was refused).
    pub(crate) fn reclassify_failed(&mut self, index: u64, key: &K) {
        self.succeeded = self.succeeded.saturating_sub(1);
        self.failures += 1;
        self.failed.insert(index, key.clone());
    }

    /// Stops admission and drops every future in flight. Retained results stay for the report.
    pub(crate) fn halt(&mut self, halt: Halt) {
        if self.halted.is_some() {
            return;
        }
        self.halted = Some(halt);
        self.active.clear();
        let open = std::mem::take(&mut self.open);
        for (index, key) in open {
            if self.ready.contains_key(&index) {
                self.open.insert(index, key);
            } else {
                self.cancelled.insert(index, key);
            }
        }
    }

    pub(crate) fn progress(&self) -> Progress {
        Progress {
            admitted: self.next_index,
            in_flight: self.active.len(),
            retained: self.ready.len(),
            emitted: self.emitted,
            succeeded: self.succeeded,
            failed: self.failures,
            cancelled: self.cancelled.len() as u64,
        }
    }

    pub(crate) fn checkpoint(&self) -> Checkpoint<K> {
        let mut unfinished =
            Vec::with_capacity(self.open.len() + self.failed.len() + self.cancelled.len());
        for (&index, key) in &self.open {
            let state = match self.ready.get(&index) {
                Some(Ok(_)) => Unfinished::Retained,
                // Listed once, from `failed`.
                Some(Err(_)) => continue,
                None => Unfinished::InFlight,
            };
            unfinished.push(entry(index, key.clone(), state));
        }
        for (&index, key) in &self.failed {
            unfinished.push(entry(index, key.clone(), Unfinished::Failed));
        }
        for (&index, key) in &self.cancelled {
            unfinished.push(entry(index, key.clone(), Unfinished::Cancelled));
        }
        unfinished.sort_by_key(|entry| entry.index);
        Checkpoint {
            admitted: self.next_index,
            unfinished,
        }
    }

    /// Consumes the window after the owner halted it or it ran dry.
    pub(crate) fn settle(mut self) -> Settled<K, R, E> {
        if !self.active.is_empty() {
            self.halt(Halt::Stopped);
        }
        let mut unfinished = Vec::new();
        let mut retained = Vec::new();
        let mut unemitted_errors = Vec::new();
        for (index, result) in std::mem::take(&mut self.ready) {
            let Some(key) = self.open.remove(&index) else {
                continue;
            };
            match result {
                Ok(value) => {
                    unfinished.push(entry(index, key.clone(), Unfinished::Retained));
                    retained.push(Keyed { index, key, value });
                }
                Err(value) => unemitted_errors.push(Keyed { index, key, value }),
            }
        }
        for (index, key) in std::mem::take(&mut self.failed) {
            unfinished.push(entry(index, key, Unfinished::Failed));
        }
        for (index, key) in std::mem::take(&mut self.cancelled) {
            unfinished.push(entry(index, key, Unfinished::Cancelled));
        }
        unfinished.sort_by_key(|entry| entry.index);
        Settled {
            unfinished,
            retained,
            unemitted_errors,
        }
    }
}

fn entry<K>(index: u64, key: K, value: Unfinished) -> Keyed<K, Unfinished> {
    Keyed { index, key, value }
}
