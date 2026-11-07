#![allow(missing_docs)]
use concord_ratelimit::{Budget, Clock, Error, Limiter, Mode, Observation};
use std::{
    future::Future,
    num::{NonZeroU32, NonZeroUsize},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;

#[derive(Clone)]
struct Fake {
    ms: Arc<AtomicU64>,
    changed: watch::Sender<u64>,
}
impl Fake {
    fn new() -> Self {
        let (changed, _) = watch::channel(0);
        Self {
            ms: Arc::new(AtomicU64::new(0)),
            changed,
        }
    }
    fn advance(&self, ms: u64) {
        self.ms.fetch_add(ms, Ordering::SeqCst);
        self.changed.send_replace(self.ms.load(Ordering::SeqCst));
    }
}
impl Clock for Fake {
    fn now(&self) -> Duration {
        Duration::from_millis(self.ms.load(Ordering::SeqCst))
    }
    fn sleep(&self, d: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let end = self.now().saturating_add(d);
        let this = self.clone();
        let mut rx = self.changed.subscribe();
        Box::pin(async move {
            while this.now() < end {
                rx.changed().await.unwrap();
            }
        })
    }
}
fn limiter(clock: Fake, capacity: usize) -> Limiter<&'static str> {
    Limiter::new(NonZeroUsize::new(capacity).unwrap(), clock)
}

#[tokio::test]
async fn shared_bucket_observation_and_reset() {
    let clock = Fake::new();
    let l = limiter(clock.clone(), 3);
    // The SDK maps both routes to the same bucket key.
    let routes = ["shared", "shared"];
    l.observe(
        "shared",
        Observation::Window {
            remaining: 1,
            reset_at: Duration::from_millis(500),
        },
    )
    .unwrap();
    l.acquire(&[routes[0]], Mode::Refuse).await.unwrap();
    assert_eq!(
        l.acquire(&[routes[1]], Mode::Refuse).await,
        Err(Error::Gated(Some(Duration::from_millis(500))))
    );
    clock.advance(500);
    l.acquire(&[routes[1]], Mode::Refuse).await.unwrap();
}
#[tokio::test]
async fn global_and_fractional_cooldowns() {
    let clock = Fake::new();
    let l = limiter(clock.clone(), 3);
    l.observe(
        "a",
        Observation::Cooldown(l.after(Duration::from_millis(1250)).unwrap()),
    )
    .unwrap();
    l.observe_global(l.after(Duration::from_millis(500)).unwrap());
    assert_eq!(
        l.acquire(&["b"], Mode::Refuse).await,
        Err(Error::Gated(Some(Duration::from_millis(500))))
    );
    assert_eq!(
        l.acquire(&["a"], Mode::Refuse).await,
        Err(Error::Gated(Some(Duration::from_millis(1250))))
    );
    clock.advance(500);
    l.acquire(&["b"], Mode::Refuse).await.unwrap();
    clock.advance(750);
    l.acquire(&["a"], Mode::Refuse).await.unwrap();
}
#[tokio::test]
async fn wait_refusal_and_cancel_do_not_charge() {
    let clock = Fake::new();
    let l = limiter(clock.clone(), 2);
    l.set_budget(
        "a",
        Budget {
            requests: NonZeroU32::new(1).unwrap(),
            window: Duration::from_millis(100),
        },
    )
    .unwrap();
    l.acquire(&["a"], Mode::Wait).await.unwrap();
    let clone = l.clone();
    let waiter = tokio::spawn(async move { clone.acquire(&["a"], Mode::Wait).await });
    tokio::task::yield_now().await;
    assert!(matches!(
        l.acquire(&["a"], Mode::Refuse).await,
        Err(Error::Gated(_))
    ));
    waiter.abort();
    let _ = waiter.await;
    clock.advance(100);
    l.acquire(&["a"], Mode::Refuse).await.unwrap();
    assert!(matches!(
        l.acquire(&["a"], Mode::Refuse).await,
        Err(Error::Gated(_))
    ));
}
#[tokio::test]
async fn fifo_shared_clones_and_lease_drop() {
    let l = limiter(Fake::new(), 2);
    l.set_concurrency("a", NonZeroUsize::new(1).unwrap())
        .unwrap();
    let first = l.lease("a", Mode::Refuse).await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let a = l.clone();
    let tx1 = tx.clone();
    let one = tokio::spawn(async move {
        let _p = a.lease("a", Mode::Wait).await.unwrap();
        tx1.send(1).unwrap();
    });
    tokio::task::yield_now().await;
    let a = l.clone();
    let two = tokio::spawn(async move {
        let _p = a.lease("a", Mode::Wait).await.unwrap();
        tx.send(2).unwrap();
    });
    tokio::task::yield_now().await;
    drop(first);
    assert_eq!(rx.recv().await, Some(1));
    assert_eq!(rx.recv().await, Some(2));
    one.await.unwrap();
    two.await.unwrap();
    l.lease("a", Mode::Refuse).await.unwrap();
}
#[tokio::test]
async fn wait_wakes_at_reset_and_bounded_storage() {
    let clock = Fake::new();
    let l = limiter(clock.clone(), 1);
    l.observe(
        "a",
        Observation::Window {
            remaining: 0,
            reset_at: Duration::from_millis(10),
        },
    )
    .unwrap();
    assert_eq!(l.acquire(&["b"], Mode::Refuse).await, Err(Error::Capacity));
    assert!(!l.remove(&"a"));
    let clone = l.clone();
    let task = tokio::spawn(async move { clone.acquire(&["a"], Mode::Wait).await });
    tokio::task::yield_now().await;
    clock.advance(10);
    task.await.unwrap().unwrap();
    assert!(l.remove(&"a"));
    l.acquire(&["b"], Mode::Refuse).await.unwrap();
}
#[tokio::test]
async fn overflow_and_atomic_multikey() {
    let clock = Fake::new();
    let l = limiter(clock.clone(), 2);
    assert_eq!(l.after(Duration::MAX), Ok(Duration::MAX));
    l.set_budget(
        "a",
        Budget {
            requests: NonZeroU32::new(1).unwrap(),
            window: Duration::MAX,
        },
    )
    .unwrap();
    l.observe("b", Observation::Cooldown(Duration::from_secs(1)))
        .unwrap();
    assert!(l.acquire(&["a", "b"], Mode::Refuse).await.is_err());
    l.acquire(&["a"], Mode::Refuse).await.unwrap();
    assert_eq!(
        l.acquire(&["a"], Mode::Refuse).await,
        Err(Error::Gated(Some(Duration::MAX)))
    );
    clock.advance(1);
    assert_eq!(l.after(Duration::MAX), Err(Error::Overflow));
    let fresh = limiter(clock, 1);
    fresh
        .set_budget(
            "a",
            Budget {
                requests: NonZeroU32::new(1).unwrap(),
                window: Duration::MAX,
            },
        )
        .unwrap();
    assert_eq!(
        fresh.acquire(&["a"], Mode::Refuse).await,
        Err(Error::Overflow)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_clones_never_exceed_lease_limit() {
    let l = limiter(Fake::new(), 1);
    l.set_concurrency("a", NonZeroUsize::new(1).unwrap())
        .unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(17));
    let in_flight = Arc::new(AtomicU64::new(0));
    let peak = Arc::new(AtomicU64::new(0));
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let (l, barrier, in_flight, peak) =
            (l.clone(), barrier.clone(), in_flight.clone(), peak.clone());
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            let _lease = l.lease("a", Mode::Wait).await.unwrap();
            let count = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(count, Ordering::SeqCst);
            tokio::task::yield_now().await;
            in_flight.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    barrier.wait().await;
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(peak.load(Ordering::SeqCst), 1);
}
