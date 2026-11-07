//! Explicitly keyed, process-local admission. See the crate README for composition and semantics.
#![forbid(unsafe_code)]

use std::{
    collections::{HashMap, VecDeque},
    future::{Future, poll_fn},
    hash::Hash,
    num::{NonZeroU32, NonZeroUsize},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    time::Duration,
};

/// Monotonic time and cancellable sleep supplied by the embedding runtime.
pub trait Clock: Send + Sync + 'static {
    /// Time since an arbitrary, stable epoch.
    fn now(&self) -> Duration;
    /// Sleep for at least this duration.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

/// Tokio-backed monotonic clock, available with the `tokio` feature.
#[cfg(feature = "tokio")]
pub struct TokioClock(std::time::Instant);
#[cfg(feature = "tokio")]
impl Default for TokioClock {
    fn default() -> Self {
        Self(std::time::Instant::now())
    }
}
#[cfg(feature = "tokio")]
impl Clock for TokioClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let mut left = duration;
            while !left.is_zero() {
                let chunk = left.min(Duration::from_secs(86_400));
                tokio::time::sleep(chunk).await;
                left = left.saturating_sub(chunk);
            }
        })
    }
}

/// Whether to queue or refuse when admission is unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Queue in FIFO order.
    Wait,
    /// Return immediately.
    Refuse,
}
/// Admission failure. A gated delay is a lower bound, not a reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Key storage is full.
    Capacity,
    /// Admission is blocked; `None` means no timed reopening is known.
    Gated(Option<Duration>),
    /// A time calculation exceeded `Duration`.
    Overflow,
    /// A zero-length window is invalid.
    InvalidWindow,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rate admission: {self:?}")
    }
}
impl std::error::Error for Error {}

/// A fixed-window local budget. Windows start with the first admission after expiry.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Maximum admissions per window.
    pub requests: NonZeroU32,
    /// Window length, which must be nonzero.
    pub window: Duration,
}
/// Provider information already interpreted by the SDK, using the clock's epoch.
#[derive(Clone, Copy, Debug)]
pub enum Observation {
    /// Remaining capacity and the time at which this observation expires.
    Window {
        /// Remaining requests.
        remaining: u32,
        /// Monotonic reset time.
        reset_at: Duration,
    },
    /// Close admission through this time.
    Cooldown(Duration),
}

struct Entry {
    budget: Option<Budget>,
    local: Option<(u32, Duration)>,
    observed: Option<(u32, Duration)>,
    cooldown: Duration,
    limit: Option<NonZeroUsize>,
    leased: usize,
}
impl Default for Entry {
    fn default() -> Self {
        Self {
            budget: None,
            local: None,
            observed: None,
            cooldown: Duration::ZERO,
            limit: None,
            leased: 0,
        }
    }
}
type SleepFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Waiter {
    id: Arc<()>,
    waker: Option<Waker>,
}
struct State<K> {
    entries: HashMap<K, Entry>,
    global: Duration,
    queue: VecDeque<Waiter>,
}
struct Shared<K> {
    state: Mutex<State<K>>,
    capacity: usize,
    clock: Arc<dyn Clock>,
}
/// A cheap clone sharing budgets, observations, queue and leases within this process.
pub struct Limiter<K>(Arc<Shared<K>>);
impl<K> Clone for Limiter<K> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
fn wake<K>(state: &mut State<K>) -> Vec<Waker> {
    state
        .queue
        .iter_mut()
        .filter_map(|w| w.waker.take())
        .collect()
}
fn notify(wakers: Vec<Waker>) {
    for waker in wakers {
        waker.wake();
    }
}
impl<K: Eq + Hash + Clone> Limiter<K> {
    /// Create a limiter with a mandatory bound on distinct keys.
    pub fn new(capacity: NonZeroUsize, clock: impl Clock) -> Self {
        Self(Arc::new(Shared {
            state: Mutex::new(State {
                entries: HashMap::new(),
                global: Duration::ZERO,
                queue: VecDeque::new(),
            }),
            capacity: capacity.get(),
            clock: Arc::new(clock),
        }))
    }
    /// Current clock value, useful for constructing observations.
    pub fn now(&self) -> Duration {
        self.0.clock.now()
    }
    /// Checked conversion of a relative delay into this limiter's clock epoch.
    pub fn after(&self, delay: Duration) -> Result<Duration, Error> {
        self.now().checked_add(delay).ok_or(Error::Overflow)
    }
    fn ensure(&self, state: &mut State<K>, keys: &[K]) -> Result<Vec<K>, Error> {
        let mut unique = Vec::new();
        for k in keys {
            if !unique.contains(k) {
                unique.push(k.clone());
            }
        }
        let missing = unique
            .iter()
            .filter(|k| !state.entries.contains_key(*k))
            .count();
        if missing > self.0.capacity.saturating_sub(state.entries.len()) {
            return Err(Error::Capacity);
        }
        for k in &unique {
            state.entries.entry(k.clone()).or_default();
        }
        Ok(unique)
    }
    /// Configure a local fixed-window budget for a key, resetting its local accounting.
    pub fn set_budget(&self, key: K, budget: Budget) -> Result<(), Error> {
        if budget.window.is_zero() {
            return Err(Error::InvalidWindow);
        }
        let mut s = self.0.state.lock().unwrap();
        self.ensure(&mut s, std::slice::from_ref(&key))?;
        let e = s.entries.get_mut(&key).unwrap();
        e.budget = Some(budget);
        e.local = None;
        let w = wake(&mut s);
        drop(s);
        notify(w);
        Ok(())
    }
    /// Set the maximum simultaneous leases for a key.
    pub fn set_concurrency(&self, key: K, limit: NonZeroUsize) -> Result<(), Error> {
        let mut s = self.0.state.lock().unwrap();
        self.ensure(&mut s, std::slice::from_ref(&key))?;
        s.entries.get_mut(&key).unwrap().limit = Some(limit);
        let w = wake(&mut s);
        drop(s);
        notify(w);
        Ok(())
    }
    /// Current unused lease slots for a configured key, if it has a concurrency limit.
    pub fn available_leases(&self, key: &K) -> Option<usize> {
        let s = self.0.state.lock().unwrap();
        let e = s.entries.get(key)?;
        Some(e.limit?.get().saturating_sub(e.leased))
    }
    /// Record a provider window or cooldown. Concurrent observations are combined conservatively.
    pub fn observe(&self, key: K, observation: Observation) -> Result<(), Error> {
        let now = self.now();
        let mut s = self.0.state.lock().unwrap();
        self.ensure(&mut s, std::slice::from_ref(&key))?;
        let e = s.entries.get_mut(&key).unwrap();
        match observation {
            Observation::Cooldown(until) => e.cooldown = e.cooldown.max(until),
            Observation::Window {
                remaining,
                reset_at,
            } if reset_at > now => {
                e.observed = Some(match e.observed.filter(|(_, end)| *end > now) {
                    Some((old, end)) => (old.min(remaining), end.max(reset_at)),
                    None => (remaining, reset_at),
                });
            }
            Observation::Window { .. } => {}
        }
        let w = wake(&mut s);
        drop(s);
        notify(w);
        Ok(())
    }
    /// Extend the global cooldown, which gates all keys including empty key sets.
    pub fn observe_global(&self, until: Duration) {
        let mut s = self.0.state.lock().unwrap();
        s.global = s.global.max(until);
        let w = wake(&mut s);
        drop(s);
        notify(w);
    }
    /// Remove an inactive key after its windows expire. Returns false when still active or absent.
    pub fn remove(&self, key: &K) -> bool {
        let now = self.now();
        let mut s = self.0.state.lock().unwrap();
        // Queued requests may refer to this key; conservatively retain all keys while any wait.
        if !s.queue.is_empty() {
            return false;
        }
        let removable = s.entries.get(key).is_some_and(|e| {
            e.leased == 0
                && e.cooldown <= now
                && e.local.is_none_or(|(_, t)| t <= now)
                && e.observed.is_none_or(|(_, t)| t <= now)
        });
        removable && s.entries.remove(key).is_some()
    }
    /// Atomically charge each distinct key. Cancellation while waiting makes no charge.
    pub async fn acquire(&self, keys: &[K], mode: Mode) -> Result<(), Error> {
        self.wait(keys, mode, false).await
    }
    /// Acquire one concurrency lease, released when the returned value is dropped.
    pub async fn lease(&self, key: K, mode: Mode) -> Result<Lease<K>, Error> {
        self.wait(std::slice::from_ref(&key), mode, true).await?;
        Ok(Lease {
            shared: self.0.clone(),
            key: Some(key),
        })
    }
    async fn wait(&self, keys: &[K], mode: Mode, lease: bool) -> Result<(), Error> {
        let id = Arc::new(());
        let keys = {
            let mut s = self.0.state.lock().unwrap();
            let keys = self.ensure(&mut s, keys)?;
            if mode == Mode::Wait {
                s.queue.push_back(Waiter {
                    id: id.clone(),
                    waker: None,
                });
            }
            keys
        };
        let mut guard = QueueGuard {
            shared: self.0.clone(),
            id,
            queued: mode == Mode::Wait,
        };
        let mut timer: Option<(Duration, SleepFuture)> = None;
        poll_fn(|cx| {
            let now = self.now();
            let mut s = self.0.state.lock().unwrap();
            let front = if guard.queued {
                s.queue
                    .front()
                    .is_some_and(|w| Arc::ptr_eq(&w.id, &guard.id))
            } else {
                s.queue.is_empty()
            };
            let mut until = s.global;
            let mut blocked = false;
            for key in &keys {
                let e = s.entries.get_mut(key).expect("queued keys retained");
                if lease {
                    if e.limit.is_some_and(|n| e.leased >= n.get()) {
                        blocked = true;
                    }
                } else {
                    until = until.max(e.cooldown);
                    if let Some((n, end)) = e.observed
                        && n == 0
                        && end > now
                    {
                        until = until.max(end);
                    }
                    if let Some((n, end)) = e.local
                        && n == 0
                        && end > now
                    {
                        until = until.max(end);
                    }
                }
            }
            if front && !blocked && until <= now {
                // Check every addition before making any charge.
                if !lease
                    && keys.iter().any(|k| {
                        let e = &s.entries[k];
                        e.budget.is_some_and(|b| {
                            e.local.is_none_or(|(_, end)| end <= now)
                                && now.checked_add(b.window).is_none()
                        })
                    })
                {
                    return Poll::Ready(Err(Error::Overflow));
                }
                for key in &keys {
                    let e = s.entries.get_mut(key).unwrap();
                    if lease {
                        e.leased += 1;
                    } else {
                        if let Some(b) = e.budget {
                            if e.local.is_none_or(|(_, end)| end <= now) {
                                e.local =
                                    Some((b.requests.get(), now.checked_add(b.window).unwrap()));
                            }
                            if let Some((n, _)) = &mut e.local {
                                *n -= 1;
                            }
                        }
                        if let Some((n, end)) = &mut e.observed
                            && *end > now
                        {
                            *n = n.saturating_sub(1);
                        }
                    }
                }
                if guard.queued {
                    s.queue.pop_front();
                    guard.queued = false;
                }
                let w = wake(&mut s);
                drop(s);
                notify(w);
                return Poll::Ready(Ok(()));
            }
            let delay = (until > now).then(|| until.saturating_sub(now));
            if mode == Mode::Refuse {
                return Poll::Ready(Err(Error::Gated(delay)));
            }
            if let Some(waiter) = s.queue.iter_mut().find(|w| Arc::ptr_eq(&w.id, &guard.id)) {
                waiter.waker = Some(cx.waker().clone());
            }
            drop(s);
            if front && let Some(delay) = delay {
                if timer
                    .as_ref()
                    .is_none_or(|(deadline, _)| *deadline != until)
                {
                    timer = Some((until, self.0.clock.sleep(delay)));
                }
                if timer.as_mut().unwrap().1.as_mut().poll(cx).is_ready() {
                    timer = None;
                    cx.waker().wake_by_ref();
                }
            }
            Poll::Pending
        })
        .await
    }
}
struct QueueGuard<K> {
    shared: Arc<Shared<K>>,
    id: Arc<()>,
    queued: bool,
}
impl<K> Drop for QueueGuard<K> {
    fn drop(&mut self) {
        if self.queued {
            let mut s = self.shared.state.lock().unwrap();
            s.queue.retain(|w| !Arc::ptr_eq(&w.id, &self.id));
            let w = wake(&mut s);
            drop(s);
            notify(w);
        }
    }
}
/// An owned concurrency permit; retain it until the response body or stream finishes if desired.
pub struct Lease<K: Eq + Hash> {
    shared: Arc<Shared<K>>,
    key: Option<K>,
}
impl<K: Eq + Hash> Drop for Lease<K> {
    fn drop(&mut self) {
        let mut s = self.shared.state.lock().unwrap();
        if let Some(key) = self.key.take() {
            s.entries.get_mut(&key).expect("leased key retained").leased -= 1;
        }
        let w = wake(&mut s);
        drop(s);
        notify(w);
    }
}
