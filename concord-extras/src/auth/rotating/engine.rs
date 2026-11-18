use super::*;
use futures_util::future::BoxFuture;
use std::future::{pending, poll_fn};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard, OnceLock, Weak};
use std::task::Waker;
use tokio::sync::{watch, Notify};

type Renew = dyn Fn(Exchange) -> BoxFuture<'static, Result<TokenPair, RenewalError>> + Send + Sync;
pub(super) type Format = dyn Fn(&Secret) -> Result<HeaderValue, InvalidHeaderValue> + Send + Sync;

trait Store: Send + Sync {
    fn load(&self) -> BoxFuture<'_, Result<Stored, RenewalError>>;
    fn save(&self, record: Stored) -> BoxFuture<'_, Result<(), RenewalError>>;
}
impl<S: CredentialStore> Store for S {
    fn load(&self) -> BoxFuture<'_, Result<Stored, RenewalError>> {
        CredentialStore::load(self).boxed()
    }
    fn save(&self, record: Stored) -> BoxFuture<'_, Result<(), RenewalError>> {
        CredentialStore::save(self, record).boxed()
    }
}

#[derive(Clone)]
pub(super) struct Policy {
    pub(super) margin: Duration,
    pub(super) timeout: Duration,
    pub(super) name: HeaderName,
    pub(super) format: Arc<Format>,
}
#[derive(Clone)]
pub(super) struct Version {
    pair: TokenPair,
    deadline: Option<Instant>,
    pub(super) generation: Arc<()>,
    pub(super) rejected: bool,
}
impl Version {
    // All callers hold the account lock, so time is sampled after lock acquisition.
    fn new(pair: TokenPair) -> Self {
        Self {
            deadline: pair.deadline(),
            pair,
            generation: Arc::new(()),
            rejected: false,
        }
    }
    fn fresh(&self, margin: Duration) -> bool {
        !self.rejected
            && self
                .deadline
                .is_some_and(|end| end.saturating_duration_since(Instant::now()) > margin)
    }
}
struct Flight {
    result: watch::Sender<Option<Result<(), RotationError>>>,
}
impl Flight {
    fn new() -> Self {
        Self {
            result: watch::channel(None).0,
        }
    }
    fn finish(&self, result: Result<(), RotationError>) {
        drop(self.result.send_replace(Some(result)));
    }
}
#[derive(Default)]
pub(super) struct Progress {
    pub(super) first: OnceLock<Instant>,
}

pub(super) struct State {
    pub(super) policy: Policy,
    pub(super) current: Option<Version>,
    pending: Option<Version>,
    reload: bool,
    blocked: bool,
    stopped: bool,
    flight: Option<Arc<Flight>>,
    pub(super) progress: Arc<Progress>,
    pub(super) polling: Option<Waker>,
    pub(super) driver_waker: Option<Arc<Waker>>,
}
pub(super) struct Inner {
    pub(super) origin: Option<concord_core::reqwest::Url>,
    pub(super) demand: Notify,
    state: Mutex<State>,
    store: Arc<dyn Store>,
    renew: Arc<Renew>,
}
pub(super) struct Access {
    pub(super) name: HeaderName,
    pub(super) header: HeaderValue,
    pub(super) generation: Arc<()>,
    format: Weak<Format>,
}
pub(super) struct Wait {
    deadline: Instant,
    progress: Option<Arc<Progress>>,
}

impl Inner {
    pub(super) fn new<S: CredentialStore>(base: &Base, store: S, renew: Arc<Renew>) -> Arc<Self> {
        Arc::new(Self {
            origin: concord_core::reqwest::Url::parse(&base.to_string()).ok(),
            demand: Notify::new(),
            store: Arc::new(store),
            renew,
            state: Mutex::new(State {
                policy: Policy {
                    margin: Duration::ZERO,
                    timeout: Duration::from_secs(30),
                    name: AUTHORIZATION,
                    format: Arc::new(default_format),
                },
                current: None,
                pending: None,
                reload: true,
                blocked: false,
                stopped: false,
                flight: None,
                progress: Arc::new(Progress::default()),
                polling: None,
                driver_waker: None,
            }),
        })
    }
    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    pub(super) fn stop(&self) {
        let retired = {
            let mut state = self.lock();
            state.stopped = true;
            (
                state.current.take(),
                state.pending.take(),
                state.flight.take(),
                state.polling.take(),
                state.driver_waker.take(),
            )
        };
        if let Some(flight) = &retired.2 {
            flight.finish(Err(RotationError::Stopped));
        }
        drop(retired);
        self.demand.notify_one();
    }
    fn storage_panicked(&self) {
        let retired = {
            let mut state = self.lock();
            state.stopped = true;
            state.current.take()
        };
        drop(retired);
    }

    pub(super) fn wait(&self) -> Result<Wait, RotationError> {
        let deadline = {
            let state = self.lock();
            if state.stopped {
                return Err(RotationError::Stopped);
            }
            if state.policy.timeout.is_zero() {
                return Err(RotationError::Renewal(RenewalError::InvalidPolicy));
            }
            Instant::now()
                .checked_add(state.policy.timeout)
                .ok_or(RotationError::Renewal(RenewalError::TimeOverflow))?
        };
        Ok(Wait {
            deadline,
            progress: None,
        })
    }
    pub(super) fn is_current(&self, access: &Access, wait: &Wait) -> bool {
        let state = self.lock();
        Instant::now() < wait.deadline
            && !state.stopped
            && !state.reload
            && !state.blocked
            && state.flight.is_none()
            && state.pending.is_none()
            && state.policy.name == access.name
            && Weak::ptr_eq(&access.format, &Arc::downgrade(&state.policy.format))
            && state.current.as_ref().is_some_and(|v| {
                Arc::ptr_eq(&v.generation, &access.generation) && v.fresh(state.policy.margin)
            })
    }

    pub(super) async fn access(&self, wait: &mut Wait) -> Result<Access, RotationError> {
        let recursive = poll_fn(|cx| {
            let state = self.lock();
            Poll::Ready(
                state
                    .polling
                    .as_ref()
                    .is_some_and(|driver| driver.will_wake(cx.waker())),
            )
        })
        .await;
        if recursive {
            return Err(RotationError::Renewal(RenewalError::InvalidPolicy));
        }
        let deadline = wait.deadline;
        let progress = &mut wait.progress;
        loop {
            if self.locked_expired(deadline) {
                return Err(self.wait_ended(deadline, progress.as_ref()).await);
            }
            let (cached, flight, wake_driver) = {
                let mut state = self.lock();
                if state.stopped {
                    return Err(RotationError::Stopped);
                }
                if state.policy.timeout.is_zero() {
                    return Err(RotationError::Renewal(RenewalError::InvalidPolicy));
                }
                let cached = if state.flight.is_none()
                    && state.pending.is_none()
                    && !state.reload
                    && !state.blocked
                {
                    state
                        .current
                        .as_ref()
                        .filter(|v| v.fresh(state.policy.margin))
                        .map(|v| (state.policy.clone(), v.clone()))
                } else {
                    None
                };
                if cached.is_some() {
                    (cached, None, None)
                } else {
                    if state.blocked && state.pending.is_none() && state.flight.is_none() {
                        return Err(RotationError::NeedsReauthorization);
                    }
                    if progress.is_none() {
                        *progress = Some(state.progress.clone());
                    }
                    let flight = state
                        .flight
                        .get_or_insert_with(|| Arc::new(Flight::new()))
                        .clone();
                    // Subscribe before unlocking: the driver cannot retire a demand
                    // concurrently with a call registering its interest.
                    let receiver = flight.result.subscribe();
                    (None, Some((flight, receiver)), state.driver_waker.clone())
                }
            };
            if let Some(waker) = wake_driver {
                waker.wake_by_ref();
            }
            if let Some((policy, current)) = cached {
                let token = current.pair.access();
                let result = catch_unwind(AssertUnwindSafe(|| (policy.format)(&token)));
                let prepared = match result {
                    Ok(Ok(mut value)) => {
                        value.set_sensitive(true);
                        Ok(value)
                    }
                    Ok(Err(cause)) => Err(RotationError::Renewal(RenewalError::InvalidHeader(
                        Arc::new(cause),
                    ))),
                    Err(_) => Err(RotationError::Renewal(RenewalError::Panicked)),
                };
                drop(token);
                let (expired, valid) = {
                    let state = self.lock();
                    (
                        Instant::now() >= deadline,
                        !state.stopped
                            && !state.reload
                            && state.flight.is_none()
                            && state.pending.is_none()
                            && !state.blocked
                            && Arc::ptr_eq(&policy.format, &state.policy.format)
                            && policy.name == state.policy.name
                            && state.current.as_ref().is_some_and(|v| {
                                Arc::ptr_eq(&v.generation, &current.generation)
                                    && v.fresh(state.policy.margin)
                            }),
                    )
                };
                if expired {
                    return Err(self.wait_ended(deadline, progress.as_ref()).await);
                }
                if valid {
                    return Ok(Access {
                        name: policy.name,
                        header: match prepared {
                            Ok(header) => header,
                            Err(error) => {
                                if matches!(
                                    error,
                                    RotationError::Renewal(RenewalError::InvalidHeader(_))
                                ) {
                                    let mut state = self.lock();
                                    if let Some(version) = &mut state.current {
                                        if Arc::ptr_eq(&version.generation, &current.generation) {
                                            version.rejected = true;
                                        }
                                    }
                                }
                                return Err(error);
                            }
                        },
                        generation: current.generation,
                        format: Arc::downgrade(&policy.format),
                    });
                }
                drop(prepared);
                yield_once().await;
                continue;
            }
            if let Some((_flight, mut receiver)) = flight {
                self.demand.notify_one();
                loop {
                    if self.locked_expired(deadline) {
                        return Err(self.wait_ended(deadline, progress.as_ref()).await);
                    }
                    let result = receiver.borrow().clone();
                    if let Some(result) = result {
                        // Even a completed result cannot win over a passed deadline.
                        if self.locked_expired(deadline) {
                            return Err(self.wait_ended(deadline, progress.as_ref()).await);
                        }
                        result?;
                        break;
                    }
                    let _ = tokio::time::timeout_at(deadline, receiver.changed()).await;
                    // Recheck both the deadline and the result on the next iteration.
                }
            }
        }
    }

    fn locked_expired(&self, deadline: Instant) -> bool {
        let _state = self.lock();
        Instant::now() >= deadline
    }
    async fn wait_ended(
        &self,
        deadline: Instant,
        progress: Option<&Arc<Progress>>,
    ) -> RotationError {
        let not_polled = {
            let _state = self.lock();
            progress.is_some_and(|progress| {
                progress
                    .first
                    .get()
                    .is_none_or(|polled| *polled >= deadline)
            })
        };
        if not_polled {
            return RotationError::NotPolled;
        }
        let cause =
            match tokio::time::timeout_at(deadline, pending::<std::convert::Infallible>()).await {
                Err(cause) => cause,
                Ok(never) => match never {},
            };
        RotationError::Renewal(RenewalError::TimedOut(Arc::new(cause)))
    }

    async fn load(&self, demand: Option<&Flight>) -> Result<Option<Stored>, RotationError> {
        yield_once().await;
        if demand.is_some_and(|flight| flight.result.receiver_count() == 0) {
            return Ok(None);
        }
        let result = AssertUnwindSafe(async {
            let work = self.store.load();
            let closed = async {
                if let Some(flight) = demand {
                    flight.result.closed().await;
                } else {
                    pending::<()>().await;
                }
            };
            let mut work = std::pin::pin!(work);
            let mut closed = std::pin::pin!(closed);
            poll_fn(|cx| {
                if closed.as_mut().poll(cx).is_ready() {
                    Poll::Ready(Ok(None))
                } else {
                    work.as_mut().poll(cx).map(|result| result.map(Some))
                }
            })
            .await
        })
        .catch_unwind()
        .await;
        match result {
            Ok(result) => result.map_err(RotationError::Load),
            Err(_) => {
                self.storage_panicked();
                Err(RotationError::Stopped)
            }
        }
    }
    async fn save(&self, record: Stored, demand: Option<&Flight>) -> Result<bool, RotationError> {
        yield_once().await;
        // This is the last cancellation point. Once the marker save begins,
        // loss of every caller cannot cancel the exchange or either save.
        if demand.is_some_and(|flight| flight.result.receiver_count() == 0) {
            return Ok(false);
        }
        match AssertUnwindSafe(async { self.store.save(record).await })
            .catch_unwind()
            .await
        {
            Ok(result) => result.map(|()| true).map_err(RotationError::Save),
            Err(_) => {
                self.storage_panicked();
                Err(RotationError::Stopped)
            }
        }
    }

    async fn drive(&self, demand: Option<&Flight>) -> Result<(), RotationError> {
        let (pending, reload, blocked) = {
            let state = self.lock();
            (state.pending.clone(), state.reload, state.blocked)
        };
        if let Some(pending) = pending {
            // Any failed save can be ambiguous. Reload before deciding whether to save again.
            if reload {
                match self.load(None).await? {
                    None => return Err(RotationError::Stopped),
                    Some(record) => match record {
                        Stored::Ready(pair) if pair.same_record(&pending.pair) => {
                            // Visibility does not acknowledge durability; save again.
                        }
                        Stored::Refreshing => {}
                        Stored::Ready(_) => return Err(RotationError::NeedsReauthorization),
                    },
                }
            }
            return self.commit(pending).await;
        }
        if blocked {
            return Err(RotationError::NeedsReauthorization);
        }
        if reload {
            match self.load(demand).await? {
                None => return Ok(()),
                Some(record) => match record {
                    Stored::Refreshing => {
                        let retired = {
                            let mut state = self.lock();
                            state.blocked = true;
                            state.current.take()
                        };
                        drop(retired);
                        return Err(RotationError::NeedsReauthorization);
                    }
                    Stored::Ready(pair) => {
                        if pair.refresh.is_empty() {
                            let retired = {
                                let mut state = self.lock();
                                state.blocked = true;
                                state.reload = false;
                                state.current.take()
                            };
                            drop(retired);
                            return Err(RotationError::InvalidRefreshToken);
                        }
                        let retired = {
                            let mut state = self.lock();
                            state.reload = false;
                            if state
                                .current
                                .as_ref()
                                .is_some_and(|v| v.pair.same_record(&pair))
                            {
                                None
                            } else {
                                state.current.replace(Version::new(pair))
                            }
                        };
                        drop(retired);
                    }
                },
            }
        }
        let (current, fresh) = {
            let state = self.lock();
            (
                state.current.clone(),
                state
                    .current
                    .as_ref()
                    .is_some_and(|v| v.fresh(state.policy.margin)),
            )
        };
        if fresh {
            return Ok(());
        }
        let Some(current) = current else {
            return Err(RotationError::NeedsReauthorization);
        };
        match self.save(Stored::Refreshing, demand).await {
            Ok(false) => return Ok(()),
            Ok(true) => {}
            Err(error) => {
                self.lock().reload = true;
                return Err(error);
            }
        }
        let retired = {
            let mut state = self.lock();
            state.blocked = true;
            state.current.take()
        };
        drop(retired);
        yield_once().await;
        let exchange = Exchange {
            refresh: current.pair.refresh(),
        };
        let mut work = catch_unwind(AssertUnwindSafe(|| (self.renew)(exchange)))
            .map_err(|_| RotationError::Renewal(RenewalError::Panicked))?;
        // Await a borrow, not the owned author future. Its output must reach
        // pending before dropping the completed future can run author cleanup.
        let output =
            poll_fn(
                |cx| match catch_unwind(AssertUnwindSafe(|| work.as_mut().poll(cx))) {
                    Ok(Poll::Ready(result)) => Poll::Ready(result),
                    Ok(Poll::Pending) => Poll::Pending,
                    Err(_) => Poll::Ready(Err(RenewalError::Panicked)),
                },
            )
            .await;
        let next = match output {
            Ok(pair) if !pair.refresh.is_empty() && pair.refresh != current.pair.refresh => {
                let (next, retired) = {
                    let mut state = self.lock();
                    let next = Version::new(pair);
                    (next.clone(), state.pending.replace(next))
                };
                drop(retired);
                Ok(next)
            }
            Ok(_) => Err(RotationError::InvalidRefreshToken),
            Err(error) => Err(RotationError::Renewal(error)),
        };
        let cleanup = catch_unwind(AssertUnwindSafe(|| drop(work)));
        if let (Ok(()), Err(RotationError::Renewal(error @ RenewalError::NotSent(_)))) =
            (&cleanup, &next)
        {
            return self.restore(current, error.clone()).await;
        }
        let next = next?;
        self.commit(next).await?;
        cleanup.map_err(|_| RotationError::Renewal(RenewalError::Panicked))
    }

    // The exchange stated that its request never left, so the refresh token is
    // still unused. Save the previous pair over the marker first; only an
    // acknowledged save makes it current again in memory. A refused save keeps
    // the marker and the block, as after any failed exchange.
    async fn restore(&self, previous: Version, error: RenewalError) -> Result<(), RotationError> {
        match self.save(Stored::Ready(previous.pair.clone()), None).await {
            Ok(_) => {
                let retired = {
                    let mut state = self.lock();
                    state.blocked = false;
                    state.current.replace(previous)
                };
                drop(retired);
            }
            Err(RotationError::Stopped) => return Err(RotationError::Stopped),
            Err(_) => {}
        }
        Err(RotationError::Renewal(error))
    }

    pub(super) fn hand_back(&self, cause: RenewalError) -> Option<RotationError> {
        // The guarded poll may still destroy earlier errors or a waker before
        // delivering this result. Final driver cleanup retires the backup only
        // once the hand-over result is safely outside that guard.
        let pending = self.lock().pending.clone();
        pending.map(|version| RotationError::Unsaved {
            pair: version.pair,
            cause,
        })
    }

    async fn commit(&self, next: Version) -> Result<(), RotationError> {
        if let Err(error) = self.save(Stored::Ready(next.pair.clone()), None).await {
            self.lock().reload = true;
            return Err(error);
        }
        self.install(next)
    }
    fn install(&self, next: Version) -> Result<(), RotationError> {
        let (fresh, retired) = {
            let mut state = self.lock();
            let fresh = next.fresh(state.policy.margin);
            state.reload = false;
            state.blocked = false;
            (fresh, (state.current.replace(next), state.pending.take()))
        };
        drop(retired);
        if fresh {
            Ok(())
        } else {
            Err(RotationError::Renewal(RenewalError::Expired))
        }
    }
}

pub(super) async fn run(inner: Arc<Inner>, owners: Weak<()>) -> Result<(), RotationError> {
    let mut last = Ok(());
    loop {
        let notified = inner.demand.notified();
        let (flight, retired, stopped) = {
            let mut state = inner.lock();
            let retired = if state
                .flight
                .as_ref()
                .is_some_and(|flight| flight.result.receiver_count() == 0)
            {
                state.flight.take()
            } else {
                None
            };
            (state.flight.clone(), retired, state.stopped)
        };
        drop(retired);
        if let Some(flight) = flight {
            let result = inner.drive(Some(&flight)).await;
            let retired = inner.lock().flight.take();
            flight.finish(result.clone());
            drop(retired);
            last = result;
            let stopped = { inner.lock().stopped };
            if stopped {
                return Err(inner
                    .hand_back(RenewalError::Panicked)
                    .unwrap_or(RotationError::Stopped));
            }
            yield_once().await;
        } else if stopped {
            return Err(inner
                .hand_back(RenewalError::Panicked)
                .unwrap_or(RotationError::Stopped));
        } else if owners.upgrade().is_none() {
            let pending = { inner.lock().pending.is_some() };
            if pending {
                // One final reload/save attempt, even without resource demand.
                let retry = inner.drive(None).await;
                let stopped = { inner.lock().stopped };
                let cause = if stopped {
                    RenewalError::Panicked
                } else {
                    retry
                        .as_ref()
                        .err()
                        .map(recovery_cause)
                        .unwrap_or(RenewalError::Expired)
                };
                if let Some(error) = inner.hand_back(cause) {
                    return Err(error);
                }
                return retry;
            }
            return last;
        } else {
            notified.await;
        }
    }
}

fn recovery_cause(error: &RotationError) -> RenewalError {
    match error {
        RotationError::Load(cause)
        | RotationError::Save(cause)
        | RotationError::Renewal(cause)
        | RotationError::Unsaved { cause, .. } => cause.clone(),
        _ => RenewalError::failed(error.clone()),
    }
}

async fn yield_once() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord_testkit::Script;
    use std::sync::OnceLock;
    use std::task::Wake;

    struct Probe {
        inner: Arc<OnceLock<Weak<Inner>>>,
        seen: Arc<Mutex<Vec<bool>>>,
    }
    impl Probe {
        fn record(&self) {
            let free = self
                .inner
                .get()
                .and_then(Weak::upgrade)
                .is_none_or(|inner| inner.state.try_lock().is_ok());
            self.seen.lock().unwrap().push(free);
        }
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            self.record();
        }
    }
    impl Wake for Probe {
        fn wake(self: Arc<Self>) {
            self.record();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.record();
        }
    }
    struct StoreProbe {
        record: Mutex<Stored>,
        probe: Arc<Probe>,
    }
    impl CredentialStore for StoreProbe {
        async fn load(&self) -> Result<Stored, RenewalError> {
            self.probe.record();
            Ok(self.record.lock().unwrap().clone())
        }
        async fn save(&self, record: Stored) -> Result<(), RenewalError> {
            self.probe.record();
            let old = std::mem::replace(&mut *self.record.lock().unwrap(), record);
            drop(old);
            Ok(())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c12_6_unsaved_pair_survives_waker_drop_after_work_completes() {
        struct DropWaker;
        impl Wake for DropWaker {
            fn wake(self: Arc<Self>) {
                panic!("a completed poll must only drop this waker");
            }
        }
        impl Drop for DropWaker {
            fn drop(&mut self) {
                panic!("waker destructor");
            }
        }
        struct WakerStore {
            holder: Arc<OnceLock<Weak<Inner>>>,
            saves: Arc<std::sync::atomic::AtomicUsize>,
        }
        impl CredentialStore for WakerStore {
            async fn load(&self) -> Result<Stored, RenewalError> {
                panic!("no reload needed");
            }
            async fn save(&self, _: Stored) -> Result<(), RenewalError> {
                self.saves.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let inner = self.holder.get().and_then(Weak::upgrade).unwrap();
                // Install a safe Arc waker that becomes the last reference at
                // poll_work's post-completion cleanup, without a raw waker.
                let retired = inner
                    .lock()
                    .polling
                    .replace(Waker::from(Arc::new(DropWaker)));
                drop(retired);
                Err(RenewalError::failed(std::io::Error::other("save failed")))
            }
        }
        let holder = Arc::new(OnceLock::new());
        let saves = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (auth, mut driver) = Rotating::new(
            &"https://data.test".parse().unwrap(),
            WakerStore {
                holder: holder.clone(),
                saves: saves.clone(),
            },
            |_| async {
                panic!("no exchange needed");
            },
        );
        holder.set(Arc::downgrade(&auth.inner)).unwrap();
        {
            let mut state = auth.inner.lock();
            state.reload = false;
            state.pending = Some(Version::new(TokenPair::expires_in(
                "new",
                "r1",
                Duration::from_secs(100),
            )));
        }
        drop(auth);
        let result = (&mut driver).await;
        assert!(
            matches!(result, Err(RotationError::Unsaved { ref pair, cause: RenewalError::Panicked, .. }) if &*pair.refresh().expose() == "r1"),
            "pair lost in waker cleanup: {result:?}"
        );
        assert_eq!(saves.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(matches!(driver.await, Err(RotationError::Stopped)));
    }

    #[tokio::test(start_paused = true)]
    async fn c12_8_callbacks_wakes_and_callback_drops_run_after_unlock() {
        let holder = Arc::new(OnceLock::new());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            inner: holder.clone(),
            seen: seen.clone(),
        });
        let store = StoreProbe {
            record: Mutex::new(Stored::Ready(TokenPair::expires_in(
                "old",
                "r0",
                Duration::ZERO,
            ))),
            probe: probe.clone(),
        };
        let captured = probe.clone();
        let (auth, driver) =
            Rotating::new(&"https://data.test".parse().unwrap(), store, move |_| {
                captured.record();
                let polled = captured.clone();
                async move {
                    polled.record();
                    Ok(TokenPair::expires_in("new", "r1", Duration::from_secs(100)))
                }
            });
        holder.set(Arc::downgrade(&auth.inner)).unwrap();
        let retired_capture = Probe {
            inner: holder.clone(),
            seen: seen.clone(),
        };
        let auth = auth.header(AUTHORIZATION, move |_| {
            retired_capture.record();
            Ok(HeaderValue::from_static("new"))
        });
        let current_capture = probe.clone();
        let auth = auth.header(AUTHORIZATION, move |_| {
            current_capture.record();
            Ok(HeaderValue::from_static("new"))
        });
        let script = Script::new();
        script.answer(200);
        let request = Request::new(
            concord_core::http::Method::GET,
            "https://data.test/".parse().unwrap(),
        );
        let mut call =
            Box::pin(auth.handle(request, TryInfo::new("data.get", 1), Next::to(&script)));
        let waker = Waker::from(probe.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(call.as_mut().poll(&mut cx).is_pending());
        let mut driver = Box::pin(driver);
        for _ in 0..16 {
            assert!(driver.as_mut().poll(&mut cx).is_pending());
        }
        assert!(call.as_mut().poll(&mut cx).is_ready());
        assert_eq!(script.requests().len(), 1);
        drop(call);
        drop(driver);
        drop(auth);
        drop(waker);
        drop(probe);
        let observed = seen.lock().unwrap();
        assert!(observed.len() >= 8);
        assert!(
            observed.iter().all(|free| *free),
            "author code ran under the account lock: {observed:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn c12_6_completed_rotations_retain_no_previous_generation() {
        let holder = Arc::new(OnceLock::new());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            inner: holder.clone(),
            seen,
        });
        let store = StoreProbe {
            record: Mutex::new(Stored::Ready(TokenPair::expires_in(
                "old",
                "r0",
                Duration::ZERO,
            ))),
            probe,
        };
        let generation = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (auth, driver) =
            Rotating::new(&"https://data.test".parse().unwrap(), store, move |_| {
                let n = generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    Ok(TokenPair::expires_in(
                        format!("a{n}"),
                        format!("r{}", n.saturating_add(1)),
                        Duration::from_secs(2),
                    ))
                }
            });
        holder.set(Arc::downgrade(&auth.inner)).unwrap();
        let task = tokio::spawn(driver);
        let script = Script::new();
        let mut previous: Option<Weak<()>> = None;
        for _ in 0..20 {
            script.answer(200);
            let request = Request::new(
                concord_core::http::Method::GET,
                "https://data.test/".parse().unwrap(),
            );
            auth.handle(request, TryInfo::new("data.get", 1), Next::to(&script))
                .await
                .unwrap();
            if let Some(previous) = previous {
                assert!(previous.upgrade().is_none());
            }
            previous = Some(Arc::downgrade(
                &auth.inner.lock().current.as_ref().unwrap().generation,
            ));
            tokio::time::advance(Duration::from_secs(2)).await;
        }
        assert_eq!(script.requests().len(), 20);
        drop(auth);
        task.await.unwrap().unwrap();
        assert!(previous.unwrap().upgrade().is_none());
    }
}
