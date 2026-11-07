#![allow(missing_docs)]

use concord_ratelimit::{Budget, Clock, Error, Limiter, Mode, Observation};
use std::{
    future::{Future, poll_fn},
    hash::{Hash, Hasher},
    num::{NonZeroU32, NonZeroUsize},
    pin::{Pin, pin},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    task::Poll,
    time::Duration,
};
use tokio::sync::{oneshot, watch};

const TIMEOUT: Duration = Duration::from_secs(3);

async fn pending_once<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(
            future.as_mut().poll(cx).is_pending(),
            "expected a queued wait"
        );
        Poll::Ready(())
    })
    .await;
}

struct NowPause {
    entered: oneshot::Sender<()>,
    resume: mpsc::Receiver<()>,
}

#[derive(Clone)]
struct ManualClock {
    millis: Arc<AtomicU64>,
    changed: watch::Sender<()>,
    sleeping: watch::Sender<Option<Duration>>,
    pause: Arc<Mutex<Option<NowPause>>>,
}

impl ManualClock {
    fn new() -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(0)),
            changed: watch::channel(()).0,
            sleeping: watch::channel(None).0,
            pause: Arc::new(Mutex::new(None)),
        }
    }

    fn pause_next_now(&self) -> (oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (entered, reached) = oneshot::channel();
        let (resume, paused) = mpsc::channel();
        *self.pause.lock().unwrap() = Some(NowPause {
            entered,
            resume: paused,
        });
        (reached, resume)
    }

    fn set_millis(&self, millis: u64) {
        self.millis.store(millis, Ordering::SeqCst);
        self.changed.send_replace(());
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        let pause = self.pause.lock().unwrap().take();
        if let Some(pause) = pause {
            pause.entered.send(()).unwrap();
            // A real worker thread pauses at the boundary before the admission lock.
            // Bound this synchronous wait too, so a failed test cannot hang shutdown.
            pause.resume.recv_timeout(TIMEOUT * 2).unwrap();
        }
        Duration::from_millis(self.millis.load(Ordering::SeqCst))
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let end = self.now() + duration;
        self.sleeping.send_replace(Some(end));
        let clock = self.clone();
        let mut changed = self.changed.subscribe();
        Box::pin(async move {
            while clock.now() < end {
                changed.changed().await.unwrap();
            }
        })
    }
}

async fn refusal_survives_removal(lease: bool) {
    let clock = ManualClock::new();
    let limiter = Limiter::new(NonZeroUsize::new(2).unwrap(), clock.clone());
    limiter
        .set_concurrency("target", NonZeroUsize::new(1).unwrap())
        .unwrap();
    let held = limiter.lease("held", Mode::Refuse).await.unwrap();
    let (reached, resume) = clock.pause_next_now();
    let other = limiter.clone();
    let task = tokio::spawn(async move {
        if lease {
            other.lease("target", Mode::Refuse).await.map(drop)
        } else {
            other.acquire(&["target"], Mode::Refuse).await
        }
    });
    tokio::time::timeout(TIMEOUT, reached)
        .await
        .unwrap()
        .unwrap();
    assert!(limiter.remove(&"target"));
    resume.send(()).unwrap();
    let result = tokio::time::timeout(TIMEOUT, task).await.unwrap();
    // Catch the original implementation's secondary poison panic during Drop,
    // allowing both failures to be reported without aborting the test process.
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(held)));
    assert!(result.is_ok(), "refusal panicked after removal: {result:?}");
    assert_eq!(result.unwrap(), Ok(()));
    assert!(dropped.is_ok(), "mutex poisoning cascaded into lease Drop");
    assert!(limiter.remove(&"held"));
    assert!(limiter.remove(&"target"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusal_admission_survives_removal_before_poll() {
    refusal_survives_removal(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusal_lease_survives_removal_before_poll() {
    refusal_survives_removal(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refusal_reports_capacity_if_removed_key_is_replaced() {
    for lease in [false, true] {
        let clock = ManualClock::new();
        let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), clock.clone());
        limiter
            .set_concurrency("target", NonZeroUsize::new(1).unwrap())
            .unwrap();
        let (reached, resume) = clock.pause_next_now();
        let other = limiter.clone();
        let task =
            tokio::spawn(async move { request(&other, "target", lease, Mode::Refuse).await });
        tokio::time::timeout(TIMEOUT, reached)
            .await
            .unwrap()
            .unwrap();
        assert!(limiter.remove(&"target"));
        limiter
            .set_concurrency("replacement", NonZeroUsize::new(1).unwrap())
            .unwrap();
        resume.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(TIMEOUT, task).await.unwrap().unwrap(),
            Err(Error::Capacity)
        );
        assert_eq!(limiter.available_leases(&"replacement"), Some(1));
    }
}

#[derive(Clone)]
struct PanicKey(Arc<AtomicBool>);

impl PartialEq for PanicKey {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for PanicKey {}
impl Hash for PanicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        assert!(!self.0.swap(false, Ordering::SeqCst), "injected hash panic");
        0_u8.hash(state);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poison_does_not_cascade_into_lease_drop() {
    let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), ManualClock::new());
    let key = PanicKey(Arc::new(AtomicBool::new(false)));
    limiter
        .set_concurrency(key.clone(), NonZeroUsize::new(1).unwrap())
        .unwrap();
    let lease = limiter.lease(key.clone(), Mode::Refuse).await.unwrap();
    let mut waiter = Box::pin(limiter.lease(key.clone(), Mode::Wait));
    pending_once(waiter.as_mut()).await;
    key.0.store(true, Ordering::SeqCst);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            limiter.available_leases(&key)
        }))
        .is_err()
    );
    // Catch each cleanup separately so a regression reports both poison panics
    // instead of aborting the process by panicking again during unwinding.
    let cancelled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(waiter)));
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(lease)));
    assert!(
        cancelled.is_ok(),
        "mutex poisoning cascaded into queue cancellation"
    );
    assert!(dropped.is_ok(), "mutex poisoning cascaded into lease Drop");
    assert_eq!(limiter.available_leases(&key), Some(1));
    tokio::time::timeout(TIMEOUT, limiter.acquire(&[key], Mode::Refuse))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lease_holder_can_acquire_ahead_of_a_lease_waiter() {
    let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), ManualClock::new());
    limiter
        .set_concurrency("a", NonZeroUsize::new(1).unwrap())
        .unwrap();
    let held = limiter.lease("a", Mode::Refuse).await.unwrap();
    let (queued, reached) = oneshot::channel();
    let other = limiter.clone();
    let waiter = tokio::spawn(async move {
        let mut lease = pin!(other.lease("a", Mode::Wait));
        pending_once(lease.as_mut()).await;
        queued.send(()).unwrap();
        lease.await.unwrap()
    });
    tokio::time::timeout(TIMEOUT, reached)
        .await
        .unwrap()
        .unwrap();
    let admission = tokio::time::timeout(TIMEOUT, limiter.acquire(&["a"], Mode::Wait)).await;
    drop(held);
    drop(
        tokio::time::timeout(TIMEOUT, waiter)
            .await
            .unwrap()
            .unwrap(),
    );
    assert!(
        admission.is_ok(),
        "lease holder's admission blocked behind its lease waiter: {admission:?}"
    );
    assert_eq!(admission.unwrap(), Ok(()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn composition_checks_cooldown_and_charges_after_the_lease_wait() {
    let clock = ManualClock::new();
    let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), clock.clone());
    limiter
        .set_concurrency("a", NonZeroUsize::new(1).unwrap())
        .unwrap();
    limiter
        .set_budget(
            "a",
            Budget {
                requests: NonZeroU32::new(1).unwrap(),
                window: Duration::from_millis(100),
            },
        )
        .unwrap();
    let held = limiter.lease("a", Mode::Wait).await.unwrap();
    let (queued, reached) = oneshot::channel();
    let mut sleeping = clock.sleeping.subscribe();
    let other = limiter.clone();
    let mut dispatch = tokio::spawn(async move {
        // The README composition, with a handshake exposing the concurrency wait.
        let mut lease = pin!(other.lease("a", Mode::Wait));
        pending_once(lease.as_mut()).await;
        queued.send(()).unwrap();
        let _lease = lease.await.unwrap();
        other.acquire(&["a"], Mode::Wait).await.unwrap();
        other.now() // exactly one dispatch
    });
    tokio::time::timeout(TIMEOUT, reached)
        .await
        .unwrap()
        .unwrap();
    // The lease wait outlives a local window. A then observes a provider cooldown
    // before releasing its lease, as it would while processing a 429 response.
    clock.set_millis(200);
    limiter
        .observe("a", Observation::Cooldown(Duration::from_millis(1000)))
        .unwrap();
    drop(held);
    tokio::time::timeout(TIMEOUT, async {
        tokio::select! {
            sent = &mut dispatch => panic!("dispatched during newly observed cooldown: {:?}", sent.unwrap()),
            changed = sleeping.changed() => changed.unwrap(),
        }
    })
    .await
    .unwrap();
    assert_eq!(*sleeping.borrow(), Some(Duration::from_millis(1000)));
    clock.set_millis(1000);
    assert_eq!(
        tokio::time::timeout(TIMEOUT, dispatch)
            .await
            .unwrap()
            .unwrap(),
        Duration::from_millis(1000)
    );
    // Admission charged a fresh local window at dispatch, not before the lease wait.
    assert_eq!(
        limiter.acquire(&["a"], Mode::Refuse).await,
        Err(Error::Gated(Some(Duration::from_millis(100))))
    );
    assert_eq!(limiter.available_leases(&"a"), Some(1));
}

async fn request(
    limiter: &Limiter<&'static str>,
    key: &'static str,
    lease: bool,
    mode: Mode,
) -> Result<(), Error> {
    if lease {
        limiter.lease(key, mode).await.map(drop)
    } else {
        limiter.acquire(&[key], mode).await
    }
}

async fn independent_fifo_and_cancellation(lease: bool) {
    let clock = ManualClock::new();
    let limiter = Limiter::new(NonZeroUsize::new(3).unwrap(), clock.clone());
    limiter
        .set_concurrency("head", NonZeroUsize::new(1).unwrap())
        .unwrap();
    limiter
        .set_budget(
            "head",
            Budget {
                requests: NonZeroU32::new(1).unwrap(),
                window: Duration::from_millis(10),
            },
        )
        .unwrap();
    limiter
        .observe("head", Observation::Cooldown(Duration::from_millis(100)))
        .unwrap();
    let held = limiter.lease("head", Mode::Refuse).await.unwrap();
    let mut head = Box::pin(request(&limiter, "head", lease, Mode::Wait));
    pending_once(head.as_mut()).await;
    let mut second = Box::pin(request(&limiter, "second", lease, Mode::Wait));
    pending_once(second.as_mut()).await;
    let mut third = Box::pin(request(&limiter, "third", lease, Mode::Wait));
    pending_once(third.as_mut()).await;
    assert!(!limiter.remove(&"second"));
    assert_eq!(
        request(&limiter, "second", lease, Mode::Refuse).await,
        Err(Error::Gated(None))
    );
    // Both modes in the other queue can proceed despite this blocked FIFO.
    for mode in [Mode::Refuse, Mode::Wait] {
        tokio::time::timeout(TIMEOUT, request(&limiter, "second", !lease, mode))
            .await
            .unwrap()
            .unwrap();
    }
    drop(head);
    pending_once(third.as_mut()).await; // Cancellation does not let third pass second.
    tokio::time::timeout(TIMEOUT, second)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(TIMEOUT, third).await.unwrap().unwrap();
    assert!(limiter.remove(&"second"));
    assert!(limiter.remove(&"third"));
    drop(held);
    clock.set_millis(100);
    // Cancellation charged neither a rate admission nor an extra lease.
    assert_eq!(limiter.available_leases(&"head"), Some(1));
    limiter.acquire(&["head"], Mode::Refuse).await.unwrap();
    assert_eq!(
        limiter.acquire(&["head"], Mode::Refuse).await,
        Err(Error::Gated(Some(Duration::from_millis(10))))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_fifo_refusal_and_cancellation_are_independent_of_leases() {
    independent_fifo_and_cancellation(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lease_fifo_refusal_and_cancellation_are_independent_of_admission() {
    independent_fifo_and_cancellation(true).await;
}
