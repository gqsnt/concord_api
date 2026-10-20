#![cfg(feature = "rate-limit")]

use std::future::Future as _;
use std::num::NonZeroU32;
use std::time::Duration;

use concord_core::contracts::{AttemptInfo, Method};
use concord_core::rate_limit::{
    RateLimitId, RateLimitKey, RateLimitPlan, RateLimitPolicy, RateLimitRequest, RateLimitResponse,
    RateLimitWindow, RateLimiter,
};
use concord_extras::rate_limit::LocalRateLimiter;
use http::StatusCode;

fn one_per_minute() -> RateLimitPlan {
    let window = RateLimitWindow::new(NonZeroU32::MIN, Duration::from_secs(60))
        .expect("valid finite window");
    let policy = RateLimitPolicy::new(
        RateLimitId::new("public-contract").expect("valid ID"),
        RateLimitKey::from_canonical_values([("tenant", "public-contract-tenant")])
            .expect("bounded tenant key"),
        NonZeroU32::MIN,
        [window],
    )
    .expect("valid public policy");
    RateLimitPlan::try_from_policies([policy]).expect("valid public plan")
}

#[tokio::test(start_paused = true)]
async fn downstream_local_limiter_uses_public_contracts_and_core_cooldown_cap() {
    let limiter = LocalRateLimiter::new();
    assert_eq!(limiter.preparation_key_limit(), 1_024);
    assert_eq!(one_per_minute().policies().len(), 1);

    let plan = one_per_minute();
    let request = || RateLimitRequest {
        cooldown_origin: None,
        endpoint: "RateApi::limited",
        method: &Method::GET,
        logical_origin: Some("https://example.test"),
        page_index: 0,
        attempt: AttemptInfo {
            index: 0,
            policy_retry_index: 0,
            authentication_recovery: false,
        },
        plan: &plan,
        capability: limiter.capability(),
    };
    limiter.observe(RateLimitResponse {
        request: request(),
        status: StatusCode::TOO_MANY_REQUESTS,
        server_delay: Some(Duration::from_secs(7)),
        cooldown_cap: Duration::from_secs(3),
    });

    let mut pending = Box::pin(limiter.acquire(request()));
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    assert!(pending.as_mut().poll(&mut context).is_pending());

    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert!(pending.as_mut().poll(&mut context).is_pending());

    tokio::time::advance(Duration::from_secs(1)).await;
    pending
        .await
        .expect("core-supplied three-second cap releases the public limiter");
}
