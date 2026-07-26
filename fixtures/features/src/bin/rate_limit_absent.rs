#![forbid(unsafe_code)]

fn main() {
    #[cfg(feature = "fixture-absence")]
    {
        let _ = runtime::rate_limit::LocalRateLimiter::new();
        let _ = runtime::rate_limit::RetryAfterObserver;
    }
}
