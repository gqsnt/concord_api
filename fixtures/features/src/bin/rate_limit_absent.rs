#![forbid(unsafe_code)]

fn main() {
    #[cfg(feature = "fixture-absence")]
    {
        // `LocalRateLimiter` is the only facade item the v3 `rate-limit`
        // feature gates. (v2.1 also gated `RetryAfterObserver`; v3 has no such
        // type in any configuration, so asserting its absence proved nothing.)
        let _ = runtime::rate_limit::LocalRateLimiter::new();
    }
}
