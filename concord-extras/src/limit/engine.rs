//! Reservations and reconciliation share one short lock; author code never holds it.
use super::{Bucket, Kind, LimitError, RequestInfo, Seen, Window};
use concord_core::reqwest::{Request, Response};
use concord_core::transport::TryInfo;
use std::collections::{HashMap, VecDeque};
use std::future::{poll_fn, Future};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::Poll;
use std::time::{Duration, SystemTime};
use tokio::sync::Notify;
use tokio::time::{sleep_until, Instant};

const MAX_WINDOWS: usize = 8;
const MAX_KEY_BYTES: usize = 512;
const MAX_HISTORY: usize = 4096;

type Identity = (usize, String);

#[derive(Clone)]
pub(super) struct Policy {
    pub(super) max_in_flight: Option<usize>,
    pub(super) max_hold: Duration,
    pub(super) max_keys: usize,
    pub(super) without_retry_after: Option<Duration>,
    pub(super) buckets: Vec<Bucket>,
}

pub(super) struct State {
    pub(super) policy: Policy,
    buckets: HashMap<Identity, Account>,
    active: usize,
    global: Gate,
}

pub(super) struct Inner {
    state: Mutex<State>,
    changed: Notify,
    #[cfg(test)]
    before_lock: Mutex<Option<BeforeLock>>,
}

#[cfg(test)]
type BeforeLock = Box<dyn FnOnce(&Inner) + Send>;

#[derive(Default)]
struct Gate {
    started: Option<Instant>,
    delay: Duration,
    retry_after: Option<Duration>,
}

impl Gate {
    fn remaining(&self, now: Instant) -> Duration {
        self.started.map_or(Duration::ZERO, |start| {
            self.delay
                .saturating_sub(now.saturating_duration_since(start))
        })
    }

    fn extend(&mut self, now: Instant, wait: Duration, retry_after: Option<Duration>) {
        if wait > self.remaining(now) {
            self.started = Some(now);
            self.delay = wait;
            self.retry_after = retry_after;
        }
    }

    fn inspect(&self, now: Instant, wait: &mut Wait) {
        let left = self.remaining(now);
        if !left.is_zero() {
            if left > wait.required {
                wait.required = left;
                wait.retry_after = self.retry_after;
            }
            if let Some(end) = now.checked_add(left) {
                wait.until = wait.until.max(end);
            }
        }
    }
}

struct Account {
    windows: Vec<History>,
    in_flight: u128,
    in_use: usize,
    gate: Gate,
}

impl Account {
    fn new(windows: &[Window], now: Instant) -> Self {
        Self {
            windows: windows
                .iter()
                .map(|window| History::new(window, now, true))
                .collect(),
            in_flight: 0,
            in_use: 0,
            gate: Gate::default(),
        }
    }

    fn forget_spent(&mut self, now: Instant, cost: u64) {
        if self.in_flight != 0 {
            return;
        }
        for window in &mut self.windows {
            if !window.configured
                && (window.available(now) == 0
                    || (window.remaining < cost && cost <= window.capacity))
                && window.refilled
            {
                // Keep the slot for reconciliation with other windows in the snapshot.
                window.kind = Kind::Fixed(None);
            }
        }
        if self.windows.iter().all(History::unknown) {
            self.windows.clear();
        }
    }

    // A key holds nothing exactly when it has no head/discovery or body still
    // in flight, no unexpired 429 gate, and no spent units in any window.
    fn holds_nothing(&mut self, now: Instant) -> bool {
        self.forget_spent(now, 0);
        self.in_flight == 0
            && self.in_use == 0
            && self.gate.remaining(now).is_zero()
            && self
                .windows
                .iter_mut()
                .all(|window| window.unknown() || window.available(now) == window.capacity)
    }

    fn inspect(&mut self, cost: u64, now: Instant, wait: &mut Wait) -> Result<(), LimitError> {
        self.forget_spent(now, cost);
        self.in_use
            .checked_add(1)
            .ok_or(LimitError::InvalidPolicy)?;
        self.gate.inspect(now, wait);
        self.in_flight
            .checked_add(u128::from(cost))
            .ok_or(LimitError::InvalidPolicy)?;
        for window in &mut self.windows {
            if window.unknown() {
                wait.unknown |= self.in_flight != 0;
                continue;
            }
            if cost > window.capacity {
                return Err(LimitError::CostExceedsCapacity);
            }
            if window.available(now) < cost {
                match window.ready_at(cost, now) {
                    Some(end) => wait.until = wait.until.max(end),
                    None => wait.unknown = true,
                }
            }
            if let Kind::Rolling(period) = window.kind {
                now.checked_add(period).ok_or(LimitError::InvalidPolicy)?;
            }
        }
        wait.unknown |= self.windows.is_empty() && self.in_flight != 0;
        Ok(())
    }

    fn learn(&mut self, windows: &[Window], cost: u64, now: Instant) {
        if windows.is_empty() || !valid(windows, now) {
            return;
        }
        // Validate the complete snapshot before touching any slot.
        if !self.windows.is_empty()
            && (windows.len() != self.windows.len()
                || self
                    .windows
                    .iter()
                    .zip(windows)
                    .any(|(old, new)| !old.compatible(new)))
        {
            return;
        }
        let others = self.in_flight.saturating_sub(u128::from(cost));
        if self.windows.is_empty() {
            self.windows = windows
                .iter()
                .map(|window| History::new(window, now, false))
                .collect();
        }
        for (old, new) in self.windows.iter_mut().zip(windows) {
            if old.unknown() {
                *old = History::new(new, now, false);
            }
            old.observe(new, cost, others, now);
        }
    }
}

struct History {
    capacity: u64,
    kind: Kind,
    remaining: u64,
    refilled: bool,
    configured: bool,
    events: VecDeque<(Instant, u128)>,
    used: u128,
    // The reset as the server dated it, when a reader of dates gave it.
    reset_at: Option<SystemTime>,
}

impl History {
    fn new(window: &Window, now: Instant, configured: bool) -> Self {
        let mut history = Self {
            capacity: window.capacity,
            kind: window.kind.clone(),
            remaining: window.remaining.unwrap_or(window.capacity),
            refilled: false,
            configured,
            events: VecDeque::new(),
            used: 0,
            reset_at: window.reset_at,
        };
        if let Kind::Rolling(period) = window.kind {
            if let Some(end) = now.checked_add(period) {
                history.append(
                    end,
                    u128::from(window.capacity.saturating_sub(history.remaining)),
                );
            }
        }
        history
    }

    fn unknown(&self) -> bool {
        // Public window settings reject this value; here it marks a forgotten slot.
        matches!(self.kind, Kind::Fixed(None))
    }

    fn compatible(&self, new: &Window) -> bool {
        match (&self.kind, &new.kind) {
            (Kind::Fixed(None), _) => true,
            (Kind::Rolling(old), Kind::Rolling(new)) => old == new,
            (Kind::Fixed(_), Kind::Fixed(_)) => true,
            _ => false,
        }
    }

    fn available(&mut self, now: Instant) -> u64 {
        match self.kind {
            Kind::Rolling(_) => {
                while self.events.front().is_some_and(|(end, _)| *end <= now) {
                    if let Some((_, cost)) = self.events.pop_front() {
                        self.used = self.used.saturating_sub(cost);
                    }
                }
                u64::try_from(u128::from(self.capacity).saturating_sub(self.used)).unwrap_or(0)
            }
            Kind::Fixed(reset) => {
                if !self.refilled && reset.is_some_and(|reset| reset <= now) {
                    self.remaining = self.capacity;
                    self.refilled = true;
                }
                self.remaining
            }
        }
    }

    fn ready_at(&self, cost: u64, now: Instant) -> Option<Instant> {
        match self.kind {
            Kind::Fixed(reset) => reset.filter(|reset| !self.refilled && *reset > now),
            Kind::Rolling(_) => {
                let mut used = self.used;
                for (end, debt) in &self.events {
                    used = used.saturating_sub(*debt);
                    if used.saturating_add(u128::from(cost)) <= u128::from(self.capacity) {
                        return Some(*end);
                    }
                }
                Some(now)
            }
        }
    }

    fn append(&mut self, end: Instant, cost: u128) {
        if cost == 0 {
            return;
        }
        if self.events.len() >= MAX_HISTORY {
            // Moving the oldest debt to the second expiry can only postpone admission.
            if let Some((_, oldest)) = self.events.pop_front() {
                if let Some((_, next)) = self.events.front_mut() {
                    *next = next.saturating_add(oldest);
                }
            }
        }
        self.events.push_back((end, cost));
        self.used = self.used.saturating_add(cost);
    }

    fn debit(&mut self, cost: u64, now: Instant) {
        // A newly inserted fixed window may already be at its reset.
        self.available(now);
        match self.kind {
            Kind::Rolling(period) => {
                // Every rolling deadline was preflighted under this same lock.
                if let Some(end) = now.checked_add(period) {
                    self.append(end, u128::from(cost));
                }
            }
            Kind::Fixed(Some(_)) => self.remaining = self.remaining.saturating_sub(cost),
            Kind::Fixed(None) => {}
        }
    }

    fn observe(&mut self, new: &Window, cost: u64, others: u128, now: Instant) {
        // Give a refill that is due before deciding whether it has been given.
        self.available(now);
        let mut same_reset = None;
        if let (Kind::Fixed(Some(old)), Kind::Fixed(Some(at))) = (&self.kind, &new.kind) {
            match (self.reset_at, new.reset_at) {
                // A reset that the server dated: its date, not the instant that this
                // response's own clock gives, says whether it is older, the same or newer.
                (Some(known), Some(seen)) if seen < known => return,
                (Some(known), Some(seen)) if seen == known => {
                    // Its refill has been given: what this late head reports is spent before it.
                    if self.refilled {
                        return;
                    }
                    same_reset = Some((*old).max(*at));
                }
                _ => {
                    if at < old || *at <= now {
                        return;
                    }
                }
            }
        }
        let target = u128::from(
            new.remaining
                .unwrap_or_else(|| new.capacity.saturating_sub(cost)),
        )
        .saturating_sub(others);
        let target = u64::try_from(target).unwrap_or(0);
        match new.kind {
            Kind::Rolling(period) => {
                let previous = self.available(now);
                self.capacity = new.capacity;
                let available = self.available(now);
                let target = target.min(previous).min(available);
                if let Some(end) = now.checked_add(period) {
                    self.append(end, u128::from(available.saturating_sub(target)));
                }
            }
            Kind::Fixed(reset) => {
                let remaining = self.available(now);
                let previous = remaining.min(
                    new.capacity
                        .saturating_sub(self.capacity.saturating_sub(remaining)),
                );
                self.capacity = new.capacity;
                self.kind = Kind::Fixed(same_reset.or(reset));
                self.reset_at = new.reset_at;
                self.refilled = false;
                self.remaining = previous.min(target).min(new.capacity);
            }
        }
    }
}

fn valid(windows: &[Window], now: Instant) -> bool {
    windows.len() <= MAX_WINDOWS
        && windows.iter().all(|window| {
            window.capacity > 0
                && window.remaining.is_none_or(|n| n <= window.capacity)
                && match window.kind {
                    Kind::Rolling(period) => !period.is_zero() && now.checked_add(period).is_some(),
                    Kind::Fixed(reset) => reset.is_some(),
                }
        })
}

pub(super) struct Selected {
    id: Identity,
    definition: Bucket,
    cost: u64,
}

struct Wait {
    until: Instant,
    required: Duration,
    unknown: bool,
    retry_after: Option<Duration>,
}

pub(super) struct Reservation {
    inner: Arc<Inner>,
    selected: Vec<Selected>,
    without_retry_after: Option<Duration>,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut state = self.inner.lock();
        for selected in &self.selected {
            if let Some(account) = state.buckets.get_mut(&selected.id) {
                account.in_flight = account.in_flight.saturating_sub(u128::from(selected.cost));
            }
        }
        drop(state);
        self.inner.changed.notify_waiters();
    }
}

pub(super) struct Permit {
    inner: Arc<Inner>,
    keys: Vec<Identity>,
    concurrency: bool,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.inner.lock();
        if self.concurrency {
            state.active = state.active.saturating_sub(1);
        }
        for key in &self.keys {
            if let Some(account) = state.buckets.get_mut(key) {
                account.in_use = account.in_use.saturating_sub(1);
            }
        }
        drop(state);
        self.inner.changed.notify_waiters();
    }
}

impl Inner {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(State {
                policy: Policy {
                    max_in_flight: None,
                    max_hold: Duration::from_secs(60),
                    max_keys: 1024,
                    without_retry_after: None,
                    buckets: Vec::new(),
                },
                buckets: HashMap::new(),
                active: 0,
                global: Gate::default(),
            }),
            changed: Notify::new(),
            #[cfg(test)]
            before_lock: Mutex::new(None),
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        #[cfg(test)]
        {
            let before = self
                .before_lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(before) = before {
                before(self);
            }
        }
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn select(
        &self,
        request: &Request,
        info: &TryInfo,
    ) -> Result<Vec<Selected>, LimitError> {
        let policy = self.lock().policy.clone();
        let now = Instant::now();
        if policy.max_keys == 0
            || policy.max_in_flight == Some(0)
            || !policy
                .buckets
                .iter()
                .all(|bucket| valid(&bucket.windows, now))
        {
            return Err(LimitError::InvalidPolicy);
        }
        let context = RequestInfo { request, info };
        let mut selected = Vec::new();
        for (index, definition) in policy.buckets.into_iter().enumerate() {
            let Some(key) = (definition.key)(&context) else {
                continue;
            };
            let cost = (definition.cost)(&context);
            if cost == 0 {
                continue;
            }
            if key.len() > MAX_KEY_BYTES {
                return Err(LimitError::InvalidPolicy);
            }
            selected.push(Selected {
                id: (index, key),
                definition,
                cost,
            });
        }
        Ok(selected)
    }

    pub(super) async fn reserve(
        self: &Arc<Self>,
        selected: Vec<Selected>,
    ) -> Result<(Reservation, Option<Permit>), LimitError> {
        let start = Instant::now();
        let max_hold = self.lock().policy.max_hold;
        let deadline = start
            .checked_add(max_hold)
            .ok_or(LimitError::InvalidPolicy)?;
        let mut waited_once = false;
        let mut pending = HashMap::new();
        loop {
            // Enable before inspection so neither a head nor a drop can lose a wakeup.
            let mut changed = std::pin::pin!(self.changed.notified());
            changed.as_mut().enable();
            let (now, wait, expired) = {
                let mut state = self.lock();
                let now = Instant::now();
                let expired = now > deadline && (waited_once || !max_hold.is_zero());
                let missing = selected
                    .iter()
                    .filter(|s| !state.buckets.contains_key(&s.id))
                    .count();
                if selected.len() > state.policy.max_keys {
                    return Err(LimitError::TooManyKeys);
                }
                let mut needed = state
                    .buckets
                    .len()
                    .saturating_add(missing)
                    .saturating_sub(state.policy.max_keys);
                if needed != 0 {
                    state.buckets.retain(|key, account| {
                        if needed != 0
                            && !selected.iter().any(|selected| &selected.id == key)
                            && account.holds_nothing(now)
                        {
                            needed = needed.saturating_sub(1);
                            false
                        } else {
                            true
                        }
                    });
                    if needed != 0 {
                        return Err(LimitError::TooManyKeys);
                    }
                }
                let mut wait = Wait {
                    until: now,
                    required: Duration::ZERO,
                    unknown: false,
                    retry_after: None,
                };
                state.global.inspect(now, &mut wait);
                wait.unknown |= state
                    .policy
                    .max_in_flight
                    .is_some_and(|cap| state.active >= cap);
                for selected in &selected {
                    match state.buckets.get_mut(&selected.id) {
                        Some(account) => account.inspect(selected.cost, now, &mut wait)?,
                        None => pending
                            .entry(selected.id.clone())
                            .or_insert_with(|| Account::new(&selected.definition.windows, now))
                            .inspect(selected.cost, now, &mut wait)?,
                    }
                }
                if !expired && !wait.unknown && wait.until <= now && wait.required.is_zero() {
                    for selected in &selected {
                        let account =
                            state.buckets.entry(selected.id.clone()).or_insert_with(|| {
                                // PANIC-SAFETY: HashMap::remove returns an Option; it does not index or use unchecked arithmetic.
                                pending.remove(&selected.id).unwrap_or_else(|| {
                                    Account::new(&selected.definition.windows, start)
                                })
                            });
                        for window in &mut account.windows {
                            window.debit(selected.cost, now);
                        }
                        account.in_flight =
                            account.in_flight.saturating_add(u128::from(selected.cost));
                        account.in_use = account.in_use.saturating_add(1);
                    }
                    let concurrency = state.policy.max_in_flight.is_some();
                    let permit = if concurrency || !selected.is_empty() {
                        if concurrency {
                            state.active = state.active.saturating_add(1);
                        }
                        Some(Permit {
                            inner: self.clone(),
                            keys: selected
                                .iter()
                                .map(|selected| selected.id.clone())
                                .collect(),
                            concurrency,
                        })
                    } else {
                        None
                    };
                    return Ok((
                        Reservation {
                            inner: self.clone(),
                            selected,
                            without_retry_after: state.policy.without_retry_after,
                        },
                        permit,
                    ));
                }
                (now, wait, expired)
            };
            let known = wait.required.max(wait.until.saturating_duration_since(now));
            let excessive = known > deadline.saturating_duration_since(now);
            if expired || now >= deadline || excessive {
                return Err(LimitError::MaxHold {
                    required: if wait.unknown && !excessive {
                        None
                    } else {
                        Some(known)
                    },
                    retry_after: wait.retry_after,
                    waited: now.saturating_duration_since(start),
                });
            }
            let wake = if wait.unknown {
                deadline
            } else {
                wait.until.min(deadline)
            };
            waited_once = true;
            let mut timer = std::pin::pin!(sleep_until(wake));
            poll_fn(|cx| {
                if timer.as_mut().poll(cx).is_ready() || changed.as_mut().poll(cx).is_ready() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
    }

    pub(super) fn observe(&self, reservation: &Reservation, response: &Response) {
        let wall = SystemTime::now();
        let retry_after = (response.status() == 429)
            .then(|| concord_core::Headers::from(response.headers().clone()).retry_after())
            .flatten();
        let hold = if response.status() == 429 {
            retry_after.or(reservation.without_retry_after)
        } else {
            None
        };
        // Invoke all author code before locking, even when it unwinds.
        let mut panicked = false;
        let proposals: Vec<_> = reservation
            .selected
            .iter()
            .map(|selected| {
                let seen = Seen {
                    key: &selected.id.1,
                    headers: response.headers(),
                    status: response.status(),
                    now: wall,
                };
                let windows =
                    selected.definition.learn.as_ref().and_then(|learn| {
                        match catch_unwind(AssertUnwindSafe(|| learn(&seen))) {
                            Ok(windows) => Some(windows),
                            Err(_) => {
                                panicked = true;
                                None
                            }
                        }
                    });
                let hit = hold.is_some()
                    && selected.definition.hit.as_ref().is_none_or(|hit| {
                        match catch_unwind(AssertUnwindSafe(|| hit(&seen))) {
                            Ok(hit) => hit,
                            Err(_) => {
                                panicked = true;
                                true
                            }
                        }
                    });
                (selected, windows, hit)
            })
            .collect();
        let mut state = self.lock();
        let now = Instant::now();
        let mut any_hit = false;
        for (selected, windows, hit) in proposals {
            if let Some(account) = state.buckets.get_mut(&selected.id) {
                if !panicked {
                    if let Some(windows) = windows {
                        account.learn(&windows, selected.cost, now);
                    }
                }
                if hit {
                    if let Some(wait) = hold {
                        account.gate.extend(now, wait, retry_after);
                    }
                    any_hit = true;
                }
            }
        }
        if !any_hit {
            if let Some(wait) = hold {
                state.global.extend(now, wait, retry_after);
            }
        }
        drop(state);
        self.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_core::transport::Transport;
    use concord_core::{Middleware, Next};
    use concord_testkit::Script;
    use futures_util::FutureExt;

    #[tokio::test(start_paused = true)]
    async fn c11_4_late_429_never_shortens_a_newer_hold() {
        let script = Script::new();
        script.answer(429).header("retry-after", "11");
        script.answer(429).header("retry-after", "10");
        script.answer(200);
        let limit = super::super::Limit::new();
        let start = Instant::now();
        let make_request = || {
            Request::new(
                concord_core::http::Method::GET,
                "https://one.test/data".parse().unwrap(),
            )
        };
        let (older, _) = limit.inner.reserve(vec![]).await.unwrap();
        let (newer, _) = limit.inner.reserve(vec![]).await.unwrap();
        let first = script
            .send(make_request(), TryInfo::new("a", 1))
            .await
            .unwrap();
        let peer = script.clone();
        *limit.inner.before_lock.lock().unwrap() = Some(Box::new(move |inner| {
            // advance updates the paused clock before its first yield. Dropping
            // this pending future models preemption, without starting a task.
            assert!(tokio::time::advance(Duration::from_secs(2))
                .now_or_never()
                .is_none());
            let response = peer
                .send(make_request(), TryInfo::new("b", 1))
                .now_or_never()
                .unwrap()
                .unwrap();
            inner.observe(&newer, &response);
        }));
        limit.inner.observe(&older, &first);
        limit
            .handle(make_request(), TryInfo::new("c", 1), Next::to(&script))
            .await
            .unwrap();
        let requests = script.requests();
        assert_eq!(requests.len(), 3);
        assert!(
            requests[2].at() >= requests[1].at() + Duration::from_secs(10),
            "the older 429 shortened the newer gate: sent at {:?}, required at least {:?}",
            requests[2].at() - start,
            requests[1].at() - start + Duration::from_secs(10)
        );
        assert_eq!(
            requests.iter().map(|r| r.at() - start).collect::<Vec<_>>(),
            [
                Duration::ZERO,
                Duration::from_secs(2),
                Duration::from_secs(13)
            ]
        );
        assert!(requests.iter().all(|r| r.info().number() == 1));
    }

    #[tokio::test(start_paused = true)]
    async fn c11_5_admission_samples_time_after_lock() {
        let script = Script::new();
        script.answer(200);
        script.answer(200);
        let limit = super::super::Limit::new().bucket(
            Bucket::new(|_| Some("k".into())).window(Window::rolling(1, Duration::from_secs(1))),
        );
        let request = || {
            Request::new(
                concord_core::http::Method::GET,
                "https://one.test/data".parse().unwrap(),
            )
        };
        let info = TryInfo::new("a", 1);
        let selected = limit.inner.select(&request(), &info).unwrap();
        let start = Instant::now();
        // The first lock reads policy; preempt at the following admission lock.
        *limit.inner.before_lock.lock().unwrap() = Some(Box::new(|inner| {
            *inner.before_lock.lock().unwrap() = Some(Box::new(|_| {
                assert!(tokio::time::advance(Duration::from_secs(2))
                    .now_or_never()
                    .is_none());
            }));
        }));
        let (reservation, permit) = limit.inner.reserve(selected).await.unwrap();
        let response = script.send(request(), info).await.unwrap();
        limit.inner.observe(&reservation, &response);
        drop((response, reservation, permit));
        limit
            .handle(request(), TryInfo::new("b", 1), Next::to(&script))
            .await
            .unwrap();
        let requests = script.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests.iter().map(|r| r.at() - start).collect::<Vec<_>>(),
            [Duration::from_secs(2), Duration::from_secs(3)]
        );
        assert!(requests.iter().all(|r| r.info().number() == 1));
    }

    #[tokio::test(start_paused = true)]
    async fn c11_6_retained_key_memory_is_bounded() {
        let windows = vec![
            Window {
                capacity: u64::MAX,
                kind: Kind::Rolling(Duration::from_secs(1)),
                remaining: None,
                reset_at: None
            };
            MAX_WINDOWS
        ];
        let now = Instant::now();
        let mut account = Account::new(&windows, now);
        for window in &mut account.windows {
            for _ in 0..MAX_HISTORY.saturating_mul(2) {
                window.append(now, 1);
            }
            assert_eq!(window.events.len(), MAX_HISTORY);
            assert!(window.events.capacity() <= MAX_HISTORY);
        }
        assert!(account.windows.capacity() <= MAX_WINDOWS);
        let entry = std::mem::size_of::<(Identity, Account)>();
        let history = std::mem::size_of::<History>();
        let debit = std::mem::size_of::<(Instant, u128)>();
        let bound = MAX_KEY_BYTES + entry + MAX_WINDOWS * (history + MAX_HISTORY * debit);
        #[cfg(all(target_pointer_width = "64", target_os = "linux"))]
        assert_eq!((entry, history, debit, bound), (128, 112, 32, 1_050_112));
        assert!(bound >= MAX_KEY_BYTES);
    }

    #[tokio::test(start_paused = true)]
    async fn c11_7_admission_lock_delay_counts_against_max_hold() {
        let limit = super::super::Limit::new().max_hold(Duration::from_secs(1));
        let script = Script::new();
        let start = Instant::now();
        *limit.inner.before_lock.lock().unwrap() = Some(Box::new(|_| {
            assert!(tokio::time::advance(Duration::from_secs(2))
                .now_or_never()
                .is_none());
        }));
        let result = limit.inner.reserve(vec![]).await;
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("admission outlived max_hold while taking the policy lock"),
        };
        assert!(
            matches!(error, LimitError::MaxHold { required: Some(required), waited, .. }
            if required.is_zero() && waited == Duration::from_secs(2))
        );
        assert_eq!(start.elapsed(), Duration::from_secs(2));
        assert_eq!(script.requests().len(), 0);
    }
}
