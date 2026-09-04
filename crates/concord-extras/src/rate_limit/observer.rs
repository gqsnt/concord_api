use concord_core::rate_limit::{Cooldown, CooldownScope, RateLimitObserver, RateLimitResponseHead};
use http::StatusCode;

/// Built-in final-429 `Retry-After` observer.
///
/// Core supplies the one parsed standard delay fact. Past, malformed, and zero
/// values are ignored. Positive values target current buckets,
/// or the logical host when the effective plan is empty. The local limiter
/// clamps every proposal to the runtime cap supplied by core. Supplying
/// [`LocalRateLimiter::observer`](super::LocalRateLimiter::observer) replaces
/// this policy unless the custom observer delegates back to it.
#[derive(Default)]
pub struct RetryAfterObserver;

impl RateLimitObserver for RetryAfterObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        if response.status != StatusCode::TOO_MANY_REQUESTS {
            return None;
        }
        let duration = response.server_delay.filter(|delay| !delay.is_zero())?;
        let scope = if response.plan.is_empty() {
            CooldownScope::Host
        } else {
            CooldownScope::CurrentBuckets
        };
        Some(Cooldown { duration, scope })
    }
}

#[cfg(test)]
mod tests {
    use concord_core::contracts::{HeaderMap, Method, SanitizedHeaders};
    use concord_core::rate_limit::{
        RateLimitId, RateLimitKey, RateLimitPlan, RateLimitPolicy, RateLimitWindow,
    };
    use std::num::NonZeroU32;
    use std::time::Duration;

    use super::*;

    #[test]
    fn default_observer_uses_only_final_429_and_sanitized_retry_after() {
        let method = Method::GET;
        let plan = RateLimitPlan::empty();
        let headers = HeaderMap::new();
        let response = RateLimitResponseHead {
            endpoint: "RateApi::get",
            method: &method,
            observed_url: "https://example.test/get",
            host: Some("example.test"),
            page_index: 0,
            attempt: concord_core::contracts::AttemptInfo {
                index: 0,
                policy_retry_index: 0,
                authentication_recovery: false,
            },
            plan: &plan,
            status: StatusCode::TOO_MANY_REQUESTS,
            headers: SanitizedHeaders::new(&headers),
            server_delay: Some(Duration::from_secs(7)),
        };
        let cooldown = RetryAfterObserver
            .observe(&response)
            .expect("positive 429 cooldown");
        assert_eq!(cooldown.duration, Duration::from_secs(7));
        assert!(matches!(cooldown.scope, CooldownScope::Host));

        let nonempty = RateLimitPlan::try_from_policies([RateLimitPolicy::new(
            RateLimitId::new("application").expect("ID"),
            RateLimitKey::global(),
            NonZeroU32::MIN,
            [RateLimitWindow::new(NonZeroU32::MIN, Duration::from_secs(1)).expect("window")],
        )
        .expect("policy")])
        .expect("plan");
        let bucket_response = RateLimitResponseHead {
            plan: &nonempty,
            ..response
        };
        let cooldown = RetryAfterObserver
            .observe(&bucket_response)
            .expect("nonempty-plan cooldown");
        assert!(matches!(cooldown.scope, CooldownScope::CurrentBuckets));

        let response = RateLimitResponseHead {
            status: StatusCode::OK,
            ..bucket_response
        };
        assert!(RetryAfterObserver.observe(&response).is_none());
    }
}
