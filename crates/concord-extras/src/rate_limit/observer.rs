use std::time::{Duration, SystemTime};

use concord_core::contracts::SanitizedHeaderValue;
use concord_core::rate_limit::{Cooldown, CooldownScope, RateLimitObserver, RateLimitResponseHead};
use http::StatusCode;
use http::header::RETRY_AFTER;

/// Default final-429 `Retry-After` observer.
///
/// Decimal delta-seconds and HTTP dates are accepted after trimming. Malformed,
/// past, and zero values are ignored. Positive values target current buckets,
/// or the logical host when the effective plan is empty. The local limiter clamps
/// every proposal to the runtime cap supplied by core.
#[derive(Default)]
pub struct RetryAfterObserver;

impl RateLimitObserver for RetryAfterObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        if response.status != StatusCode::TOO_MANY_REQUESTS {
            return None;
        }
        let SanitizedHeaderValue::Text(value) = response.headers.get(&RETRY_AFTER)? else {
            return None;
        };
        let duration = parse_positive_retry_after(value, SystemTime::now())?;
        let scope = if response.plan.is_empty() {
            CooldownScope::Host
        } else {
            CooldownScope::CurrentBuckets
        };
        Some(Cooldown { duration, scope })
    }
}

fn parse_positive_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    let duration = if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        let seconds = value.bytes().try_fold(0_u64, |seconds, byte| {
            seconds.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
        })?;
        Duration::from_secs(seconds)
    } else {
        let date = httpdate::parse_http_date(value).ok()?;
        date.duration_since(now).unwrap_or(Duration::ZERO)
    };
    (!duration.is_zero()).then_some(duration)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::time::UNIX_EPOCH;

    use concord_core::contracts::{HeaderMap, HeaderValue, Method, SanitizedHeaders};
    use concord_core::rate_limit::{
        RateLimitId, RateLimitKey, RateLimitPlan, RateLimitRule, RateLimitWindow,
    };

    use super::*;

    #[test]
    fn retry_after_parser_accepts_positive_delta_and_date_only() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(
            parse_positive_retry_after(" 12 ", now),
            Some(Duration::from_secs(12))
        );
        assert_eq!(parse_positive_retry_after("0", now), None);
        let future = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(
            parse_positive_retry_after(&format!(" {future} "), now),
            Some(Duration::from_secs(30))
        );
        let past = httpdate::fmt_http_date(now - Duration::from_secs(1));
        assert_eq!(parse_positive_retry_after(&past, now), None);
        for malformed in ["", "-1", "+1", "1.5", "tomorrow", "12 seconds"] {
            assert_eq!(parse_positive_retry_after(malformed, now), None);
        }
    }

    #[test]
    fn default_observer_uses_only_final_429_and_sanitized_retry_after() {
        let method = Method::GET;
        let plan = RateLimitPlan::empty();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static(" 7 "));
        let response = RateLimitResponseHead {
            endpoint: "RateApi::get",
            method: &method,
            safe_url: "https://example.test/get",
            host: Some("example.test"),
            page_index: 0,
            plan: &plan,
            status: StatusCode::TOO_MANY_REQUESTS,
            headers: SanitizedHeaders::new(&headers),
        };
        let cooldown = RetryAfterObserver
            .observe(&response)
            .expect("positive 429 cooldown");
        assert_eq!(cooldown.duration, Duration::from_secs(7));
        assert!(matches!(cooldown.scope, CooldownScope::Host));

        let nonempty = RateLimitPlan::try_from_rules([RateLimitRule::new(
            RateLimitId::new("application").expect("ID"),
            RateLimitKey::global(),
            NonZeroU32::MIN,
            [RateLimitWindow::new(NonZeroU32::MIN, Duration::from_secs(1)).expect("window")],
        )
        .expect("rule")])
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
