//! Deterministic credential and cache contract tests.
use std::{
    future::{Future, poll_fn},
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use concord_auth::{RefreshError, Token, TokenCache, bearer};
use tokio::{
    sync::{Semaphore, oneshot},
    task::JoinHandle,
};

type Cache = TokenCache<&'static str, String>;

fn token(value: &str, seconds: u64) -> Token {
    Token::new(bearer(value).unwrap(), Duration::from_secs(seconds))
}

fn cache(margin: u64) -> (Arc<Cache>, Arc<Mutex<Instant>>) {
    let clock = Arc::new(Mutex::new(Instant::now()));
    let now = clock.clone();
    (
        Arc::new(Cache::with_clock(
            NonZeroUsize::new(4).unwrap(),
            Duration::from_secs(margin),
            move || *now.lock().unwrap(),
        )),
        clock,
    )
}

fn advance(clock: &Mutex<Instant>, seconds: u64) {
    let mut time = clock.lock().unwrap();
    *time += Duration::from_secs(seconds);
}

// A real task, with a receipt AFTER its first poll. Tests release acquisition only after all
// callers have actually joined its shared future (a start barrier alone would race).
fn call<F, Fut>(
    cache: Arc<Cache>,
    key: &'static str,
    acquire: F,
) -> (
    JoinHandle<Result<concord_auth::Snapshot, RefreshError<String>>>,
    oneshot::Receiver<()>,
)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<Token, String>> + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut tx = Some(tx);
        let mut future = Box::pin(cache.get_or_refresh(key, acquire));
        poll_fn(|cx| {
            let result = future.as_mut().poll(cx);
            if let Some(tx) = tx.take() {
                let _ = tx.send(());
            }
            result
        })
        .await
    });
    (task, rx)
}

#[tokio::test]
async fn cold_then_cached_does_not_invoke_losing_closure() {
    let (cache, _) = cache(0);
    assert!(cache.get(&"scope").is_none());
    let first = cache
        .get_or_refresh("scope", || async { Ok(token("first", 60)) })
        .await
        .unwrap();
    let cached = cache
        .get_or_refresh("scope", || async { panic!("must not acquire") })
        .await
        .unwrap();
    assert_eq!(cached.generation(), first.generation());
    assert_eq!(cache.get(&"scope").unwrap().header(), "Bearer first");
}

#[tokio::test]
async fn expiry_is_exclusive_at_the_exact_boundary() {
    let (cache, clock) = cache(0);
    let first = cache
        .get_or_refresh("scope", || async { Ok(token("first", 60)) })
        .await
        .unwrap();
    advance(&clock, 59);
    assert!(cache.get(&"scope").is_some());
    advance(&clock, 1);
    assert!(cache.get(&"scope").is_none());
    let next = cache
        .get_or_refresh("scope", || async { Ok(token("next", 60)) })
        .await
        .unwrap();
    assert_ne!(first.generation(), next.generation());
}

#[tokio::test]
async fn margin_refreshes_early_at_exact_boundary() {
    let (cache, clock) = cache(10);
    cache
        .get_or_refresh("scope", || async { Ok(token("first", 60)) })
        .await
        .unwrap();
    advance(&clock, 49);
    assert!(cache.get(&"scope").is_some());
    advance(&clock, 1);
    assert!(cache.get(&"scope").is_none());
    assert_eq!(
        cache
            .get_or_refresh("scope", || async { Ok(token("next", 60)) })
            .await
            .unwrap()
            .header(),
        "Bearer next"
    );
}

#[tokio::test]
async fn zero_lifetime_and_large_margin_are_not_reused() {
    for (margin, lifetime) in [(0, 0), (60, 60), (61, 60)] {
        let (cache, _) = cache(margin);
        let first = cache
            .get_or_refresh("scope", move || async move { Ok(token("first", lifetime)) })
            .await
            .unwrap();
        assert!(cache.get(&"scope").is_none());
        let second = cache
            .get_or_refresh(
                "scope",
                move || async move { Ok(token("second", lifetime)) },
            )
            .await
            .unwrap();
        assert_ne!(first.generation(), second.generation());
    }
}

#[tokio::test]
async fn untrusted_lifetime_overflows_before_margin_subtraction() {
    let cache = Cache::new(NonZeroUsize::new(1).unwrap(), Duration::MAX);
    for _ in 0..2 {
        let error = cache
            .get_or_refresh("scope", || async {
                Ok(Token::new(bearer("secret").unwrap(), Duration::MAX))
            })
            .await
            .unwrap_err();
        assert!(matches!(error, RefreshError::LifetimeOverflow));
        assert!(cache.get(&"scope").is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn thirty_two_concurrent_callers_share_one_acquisition() {
    let (cache, _) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let gate = gate.clone();
        let count = acquisitions.clone();
        let (task, polled) = call(cache.clone(), "scope", move || async move {
            count.fetch_add(1, Ordering::SeqCst);
            gate.acquire().await.unwrap().forget();
            Ok(token("shared", 60))
        });
        polled.await.unwrap();
        tasks.push(task);
    }
    assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
    gate.add_permits(1);
    let mut generation = None;
    for task in tasks {
        let snapshot = task.await.unwrap().unwrap();
        assert_eq!(snapshot.header(), "Bearer shared");
        assert_eq!(
            generation.get_or_insert_with(|| snapshot.generation()),
            &snapshot.generation()
        );
    }
}

#[tokio::test]
async fn lifetime_starts_before_acquisition_io() {
    let (cache, clock) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (task, polled) = call(cache.clone(), "scope", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("aged", 60))
    });
    polled.await.unwrap();
    advance(&clock, 61);
    release.add_permits(1);
    assert_eq!(task.await.unwrap().unwrap().header(), "Bearer aged");
    assert!(cache.get(&"scope").is_none());
}

#[tokio::test]
async fn stale_401_cannot_evict_new_generation_or_pending_refresh() {
    let (cache, _) = cache(0);
    let first = cache
        .get_or_refresh("scope", || async { Ok(token("first", 60)) })
        .await
        .unwrap();
    assert!(cache.invalidate(&"scope", &first.generation()));
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (task, polled) = call(cache.clone(), "scope", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("next", 60))
    });
    polled.await.unwrap();
    assert!(!cache.invalidate(&"scope", &first.generation()));
    release.add_permits(1);
    let next = task.await.unwrap().unwrap();
    assert!(!cache.invalidate(&"scope", &first.generation()));
    assert_eq!(cache.get(&"scope").unwrap().generation(), next.generation());
    assert!(cache.invalidate(&"scope", &next.generation()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_starter_does_not_strand_waiter() {
    let (cache, _) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (starter, polled) = call(cache.clone(), "scope", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("survived", 60))
    });
    polled.await.unwrap();
    let (waiter, joined) = call(cache.clone(), "scope", || async { panic!("joined flight") });
    joined.await.unwrap();
    starter.abort();
    assert!(starter.await.unwrap_err().is_cancelled());
    release.add_permits(1);
    assert_eq!(waiter.await.unwrap().unwrap().header(), "Bearer survived");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_waiter_does_not_cancel_starter() {
    let (cache, _) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (starter, polled) = call(cache.clone(), "scope", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("survived", 60))
    });
    polled.await.unwrap();
    let (waiter, joined) = call(cache.clone(), "scope", || async { panic!("joined flight") });
    joined.await.unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    release.add_permits(1);
    assert_eq!(starter.await.unwrap().unwrap().header(), "Bearer survived");
}

#[tokio::test]
async fn all_callers_cancel_then_a_later_caller_drives_the_owned_future() {
    let (cache, _) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (starter, polled) = call(cache.clone(), "scope", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("resumed", 60))
    });
    polled.await.unwrap();
    starter.abort();
    assert!(starter.await.unwrap_err().is_cancelled());
    release.add_permits(1);
    let next = cache
        .get_or_refresh("scope", || async { panic!("must resume") })
        .await
        .unwrap();
    assert_eq!(next.header(), "Bearer resumed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_is_shared_by_all_waiters_and_next_call_retries() {
    let (cache, _) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let count = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let gate = gate.clone();
        let count = count.clone();
        let (task, joined) = call(cache.clone(), "scope", move || async move {
            count.fetch_add(1, Ordering::SeqCst);
            gate.acquire().await.unwrap().forget();
            Err("provider-secret".to_owned())
        });
        joined.await.unwrap();
        tasks.push(task);
    }
    gate.add_permits(1);
    let mut shared = None;
    for task in tasks {
        let RefreshError::Acquire(error) = task.await.unwrap().unwrap_err() else {
            panic!()
        };
        assert_eq!(error.as_str(), "provider-secret");
        assert!(Arc::ptr_eq(
            shared.get_or_insert_with(|| error.clone()),
            &error
        ));
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        cache
            .get_or_refresh("scope", || async { Ok(token("retried", 60)) })
            .await
            .unwrap()
            .header(),
        "Bearer retried"
    );
}

#[tokio::test]
async fn distinct_keys_acquire_independently_without_holding_mutex_across_io() {
    let (cache, _) = cache(0);
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (task, polled) = call(cache.clone(), "audience-a", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("a", 60))
    });
    polled.await.unwrap();
    let b = cache
        .get_or_refresh("audience-b", || async { Ok(token("b", 60)) })
        .await
        .unwrap();
    assert_eq!(b.header(), "Bearer b");
    release.add_permits(1);
    let a = task.await.unwrap().unwrap();
    assert!(!cache.invalidate(&"audience-b", &a.generation()));
}

#[tokio::test]
async fn bounded_capacity_evicts_ready_but_refuses_to_evict_acquiring_entries() {
    let cache = Arc::new(Cache::new(NonZeroUsize::new(1).unwrap(), Duration::ZERO));
    let old = cache
        .get_or_refresh("old", || async { Ok(token("old", 60)) })
        .await
        .unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (task, polled) = call(cache.clone(), "new", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("new", 60))
    });
    polled.await.unwrap();
    assert!(cache.get(&"old").is_none());
    assert!(matches!(
        cache
            .get_or_refresh("other", || async {
                panic!("capacity refused before acquisition")
            })
            .await,
        Err(RefreshError::Capacity)
    ));
    assert!(!cache.invalidate(&"new", &old.generation()));
    release.add_permits(1);
    task.await.unwrap().unwrap();
    assert!(
        cache
            .get_or_refresh("other", || async { Ok(token("other", 60)) })
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn explicit_removal_frees_abandoned_flight_and_cannot_resurrect_it() {
    let cache = Arc::new(Cache::new(NonZeroUsize::new(1).unwrap(), Duration::ZERO));
    let gate = Arc::new(Semaphore::new(0));
    let release = gate.clone();
    let (task, polled) = call(cache.clone(), "scope", move || async move {
        gate.acquire().await.unwrap().forget();
        Ok(token("obsolete", 60))
    });
    polled.await.unwrap();
    assert!(cache.remove(&"scope"));
    assert!(!cache.remove(&"scope"));
    let replacement = cache
        .get_or_refresh("scope", || async { Ok(token("replacement", 60)) })
        .await
        .unwrap();
    release.add_permits(1);
    let obsolete = task.await.unwrap().unwrap();
    assert_ne!(obsolete.generation(), replacement.generation());
    assert_eq!(
        cache.get(&"scope").unwrap().generation(),
        replacement.generation()
    );
}

#[tokio::test]
async fn redaction_covers_all_cache_types_and_untrusted_error_formatters() {
    let (cache, _) = cache(0);
    let value = Token::new(
        http::HeaderValue::from_static("secret-token-sentinel"),
        Duration::from_secs(60),
    );
    assert!(!format!("{value:?}").contains("secret-token-sentinel"));
    let snapshot = cache
        .get_or_refresh("secret-key-sentinel", || async {
            Ok(Token::new(
                http::HeaderValue::from_static("secret-token-sentinel"),
                Duration::from_secs(60),
            ))
        })
        .await
        .unwrap();
    assert!(snapshot.header().is_sensitive());
    let rendered = format!(
        "{cache:?} {snapshot:?} {:?} {:?}",
        snapshot.header(),
        snapshot.generation()
    );
    for secret in ["secret-key-sentinel", "secret-token-sentinel"] {
        assert!(!rendered.contains(secret));
    }
    for error in [
        RefreshError::Acquire(Arc::new("secret-error-sentinel")),
        RefreshError::Capacity,
        RefreshError::LifetimeOverflow,
    ] {
        assert!(!format!("{error:?} {error}").contains("secret-error-sentinel"));
        assert!(std::error::Error::source(&error).is_none());
    }
}
