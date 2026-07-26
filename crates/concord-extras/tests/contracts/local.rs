#![cfg(feature = "rate-limit")]

use std::future::Future as _;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use concord_core::contracts::{HeaderMap, Method, SanitizedHeaders};
use concord_core::rate_limit::{
    Cooldown, CooldownScope, RateLimitId, RateLimitKey, RateLimitObserver, RateLimitPlan,
    RateLimitRequest, RateLimitResponse, RateLimitResponseHead, RateLimitRule, RateLimitWindow,
    RateLimiter,
};
use concord_extras::rate_limit::LocalRateLimiter;
use http::StatusCode;

struct LongCooldown;

impl RateLimitObserver for LongCooldown {
    fn observe(&self, _response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        Some(Cooldown {
            duration: Duration::from_secs(30),
            scope: CooldownScope::Host,
        })
    }
}

fn one_per_minute() -> RateLimitPlan {
    let window = RateLimitWindow::new(NonZeroU32::MIN, Duration::from_secs(60))
        .expect("valid finite window");
    let rule = RateLimitRule::new(
        RateLimitId::new("public-contract").expect("valid ID"),
        RateLimitKey::global(),
        NonZeroU32::MIN,
        [window],
    )
    .expect("valid public rule");
    RateLimitPlan::try_from_rules([rule]).expect("valid public plan")
}

#[tokio::test(start_paused = true)]
async fn downstream_local_limiter_uses_public_contracts_and_core_cooldown_cap() {
    let limiter = LocalRateLimiter::new().observer(Arc::new(LongCooldown));
    assert_eq!(limiter.preparation_key_limit(), 1_024);
    assert_eq!(one_per_minute().rules().len(), 1);

    let plan = RateLimitPlan::empty();
    let request = || RateLimitRequest {
        endpoint: "RateApi::limited",
        method: &Method::GET,
        safe_url: "https://example.test/limited",
        host: Some("example.test"),
        page_index: 0,
        plan: &plan,
    };
    let headers = HeaderMap::new();
    limiter
        .observe(RateLimitResponse {
            request: request(),
            status: StatusCode::TOO_MANY_REQUESTS,
            headers: SanitizedHeaders::new(&headers),
            cooldown_cap: Duration::from_secs(3),
        })
        .await
        .expect("public local limiter observation");

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
