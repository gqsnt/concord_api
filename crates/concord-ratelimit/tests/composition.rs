#![allow(missing_docs)]
use concord_auth::{Token, TokenCache, bearer};
use concord_ratelimit::{Budget, Clock, Limiter, Mode};
use concord_retry::{Decision, Policy, Replay, Retry};
use std::{
    convert::Infallible,
    future::{Future, ready},
    num::{NonZeroU32, NonZeroUsize},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
struct TestClock(Arc<AtomicU64>);
impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
    fn sleep(&self, _: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test]
async fn retry_attempts_reacquire_and_token_service_has_its_own_key() {
    let clock = Arc::new(AtomicU64::new(0));
    let l = Limiter::new(NonZeroUsize::new(2).unwrap(), TestClock(clock.clone()));
    l.set_budget(
        "app",
        Budget {
            requests: NonZeroU32::new(2).unwrap(),
            window: Duration::from_secs(60),
        },
    )
    .unwrap();
    l.set_concurrency("app", NonZeroUsize::new(1).unwrap())
        .unwrap();
    l.set_concurrency("token", NonZeroUsize::new(1).unwrap())
        .unwrap();
    let cache = Arc::new(TokenCache::<&str, Infallible>::new(
        NonZeroUsize::new(1).unwrap(),
        Duration::ZERO,
    ));
    let token_calls = Arc::new(AtomicUsize::new(0));
    let waits = AtomicUsize::new(0);
    let run = Retry::new(
        Policy::new(NonZeroUsize::new(2).unwrap()),
        Replay::Permitted,
    )
    .run(
        |attempt| {
            let limiter = l.clone();
            let application = l.clone();
            let cache = cache.clone();
            let token_calls = token_calls.clone();
            async move {
                let used = cache
                    .get_or_refresh("audience", move || async move {
                        limiter.acquire(&["token"], Mode::Wait).await.unwrap();
                        let _lease = limiter.lease("token", Mode::Wait).await.unwrap();
                        token_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(Token::new(
                            bearer("synthetic").unwrap(),
                            Duration::from_secs(60),
                        ))
                    })
                    .await
                    .unwrap();
                assert!(used.header().is_sensitive());
                application.acquire(&["app"], Mode::Wait).await.unwrap();
                let _lease = application.lease("app", Mode::Wait).await.unwrap();
                attempt // replace with exactly one application send; lease drops here
            }
        },
        |attempt| {
            if *attempt == 1 {
                Decision::RetryAfter(Some(Duration::from_millis(1)))
            } else {
                Decision::Done
            }
        },
        || Duration::from_millis(clock.load(Ordering::SeqCst)),
        |delay| {
            assert_eq!(l.available_leases(&"app"), Some(1));
            waits.fetch_add(1, Ordering::SeqCst);
            clock.fetch_add(delay.as_millis() as u64, Ordering::SeqCst);
            ready(())
        },
        |_, ceiling| ceiling,
    )
    .await;
    assert_eq!(run.report.attempts, 2);
    assert_eq!(token_calls.load(Ordering::SeqCst), 1);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
    assert!(l.acquire(&["app"], Mode::Refuse).await.is_err());
}
