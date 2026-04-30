mod support;

#[test]
fn rate_limit_runtime_harness_is_available() {
    let limiter = support::FakeRateLimiter::default();
    limiter.events.record("rate_limit_acquire");
    support::assert_event_order(&limiter.events.snapshot(), &["rate_limit_acquire"]);
}
