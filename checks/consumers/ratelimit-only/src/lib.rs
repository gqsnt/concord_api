//! Runtime-free consumer of the packaged admission crate.
#[cfg(test)]
mod tests {
    use concord_ratelimit::{Clock, Limiter, Mode, Observation, Error};
    use std::{future::{Future, ready}, num::NonZeroUsize, pin::{Pin, pin}, task::{Context, Poll, Waker}, time::Duration};
    struct TestClock;
    impl Clock for TestClock {
        fn now(&self) -> Duration { Duration::ZERO }
        fn sleep(&self, _: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> { Box::pin(ready(())) }
    }
    fn run<F: Future>(f: F) -> F::Output { match pin!(f).as_mut().poll(&mut Context::from_waker(Waker::noop())) { Poll::Ready(v) => v, Poll::Pending => panic!("unexpected wait") } }
    fn limiter() -> Limiter<&'static str> { Limiter::new(NonZeroUsize::new(1).unwrap(), TestClock) }
    #[test] fn admits() { assert_eq!(run(limiter().acquire(&["a"], Mode::Refuse)), Ok(())); }
    #[test] fn gates() { let l = limiter(); l.observe("a", Observation::Cooldown(Duration::from_secs(1))).unwrap(); assert!(matches!(run(l.acquire(&["a"], Mode::Refuse)), Err(Error::Gated(_)))); }
    #[test] fn bounds_keys() { let l = limiter(); run(l.acquire(&["a"], Mode::Refuse)).unwrap(); assert_eq!(run(l.acquire(&["b"], Mode::Refuse)), Err(Error::Capacity)); }
}
