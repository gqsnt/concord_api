#[cfg(not(feature = "rate-limit-governor"))]
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use http::header::RETRY_AFTER;
use http::{Method, StatusCode};

#[cfg(any(test, not(feature = "rate-limit-governor")))]
use crate::error::RateLimitErrorKind;
use crate::hooks::BoxFuture;
use crate::redaction::{SanitizedHeaderValue, SanitizedHeaders};

use super::{RateLimitError, RateLimitPermit, RateLimitPlan};

/// Safe request metadata supplied to a selected rate limiter.
pub struct RateLimitRequest<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'static str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Sanitized logical URL.
    pub safe_url: &'a str,
    /// Final logical host when one exists.
    pub host: Option<&'a str>,
    /// Zero-based page index; direct Phase 7 execution uses zero.
    pub page_index: u32,
    /// Canonical immutable effective plan.
    pub plan: &'a RateLimitPlan,
}

/// Sanitized response-head metadata supplied to a selected rate limiter.
pub struct RateLimitResponse<'a> {
    /// The matching body-free request context.
    pub request: RateLimitRequest<'a>,
    /// Final response status for this Concord attempt.
    pub status: StatusCode,
    /// Sanitized response headers.
    pub headers: SanitizedHeaders<'a>,
}

/// Application-supplied or Concord-provided per-attempt limiter authority.
///
/// Calls may overlap. Implementations own the boundedness of their external
/// state and must treat resolved plan keys as potentially sensitive. Custom
/// implementations are responsible for protecting those values in their own
/// logs and storage.
pub trait RateLimiter: Send + Sync + 'static {
    /// Acquires authority for exactly one visible Concord attempt.
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>>;

    /// Observes one sanitized response head without replacing that response.
    fn observe<'a>(&'a self, _response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

/// The future-acquisition scope of an observed cooldown.
pub enum CooldownScope {
    /// Every exact rule/key pair in the current plan, or the host for an empty plan.
    CurrentBuckets,
    /// Every later request to the current logical host through this limiter.
    Host,
    /// Every later request through this limiter object.
    Client,
}

/// One observer-proposed future-acquisition cooldown.
pub struct Cooldown {
    /// Proposed duration before runtime clamping.
    pub duration: Duration,
    /// Affected future-acquisition scope.
    pub scope: CooldownScope,
}

/// Sanitized response-head context supplied to a governor observer.
pub struct RateLimitResponseHead<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'static str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Sanitized logical URL.
    pub safe_url: &'a str,
    /// Final logical host when one exists.
    pub host: Option<&'a str>,
    /// Zero-based page index; direct Phase 7 execution uses zero.
    pub page_index: u32,
    /// Canonical immutable effective plan.
    pub plan: &'a RateLimitPlan,
    /// Final response status for this Concord attempt.
    pub status: StatusCode,
    /// Sanitized response headers.
    pub headers: SanitizedHeaders<'a>,
}

/// Synchronous policy for translating a response head into a future cooldown.
pub trait RateLimitObserver: Send + Sync + 'static {
    /// Returns a future-acquisition cooldown, or `None` for no state change.
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown>;
}

/// The default final-429 `Retry-After` observer.
///
/// Decimal delta-seconds and HTTP dates are accepted after trimming. Malformed,
/// past, and zero values are ignored. Positive values target current buckets,
/// or the logical host when the effective plan is empty.
pub struct DefaultRateLimitObserver;

impl RateLimitObserver for DefaultRateLimitObserver {
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

/// An explicit limiter that authorizes every plan and ignores observation.
pub struct NoopRateLimiter;

#[allow(
    clippy::new_without_default,
    reason = "the normative advanced API specifies an explicit constructor without a Default contract"
)]
impl NoopRateLimiter {
    /// Creates an explicit no-op limiter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RateLimiter for NoopRateLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

#[cfg(any(test, not(feature = "rate-limit-governor")))]
pub(crate) struct FallbackRateLimiter;

#[cfg(any(test, not(feature = "rate-limit-governor")))]
impl RateLimiter for FallbackRateLimiter {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async move {
            if request.plan.is_empty() {
                Ok(RateLimitPermit::granted())
            } else {
                Err(RateLimitError::new(
                    RateLimitErrorKind::InvalidConfiguration,
                ))
            }
        })
    }
}

#[cfg(not(feature = "rate-limit-governor"))]
pub(crate) fn fallback_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(FallbackRateLimiter)
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

    use http::{HeaderMap, HeaderValue};

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
        let cooldown = DefaultRateLimitObserver
            .observe(&response)
            .expect("positive 429 cooldown");
        assert_eq!(cooldown.duration, Duration::from_secs(7));
        assert!(matches!(cooldown.scope, CooldownScope::Host));

        let nonempty = RateLimitPlan::from_rules(vec![
            super::super::RateLimitRule::new(
                super::super::RateLimitId::new("application").expect("ID"),
                super::super::RateLimitKey::global(),
                NonZeroU32::MIN,
                [
                    super::super::RateLimitWindow::new(NonZeroU32::MIN, Duration::from_secs(1))
                        .expect("window"),
                ],
            )
            .expect("rule"),
        ]);
        let bucket_response = RateLimitResponseHead {
            plan: &nonempty,
            ..response
        };
        let cooldown = DefaultRateLimitObserver
            .observe(&bucket_response)
            .expect("nonempty-plan cooldown");
        assert!(matches!(cooldown.scope, CooldownScope::CurrentBuckets));

        let response = RateLimitResponseHead {
            status: StatusCode::OK,
            ..bucket_response
        };
        assert!(DefaultRateLimitObserver.observe(&response).is_none());
    }

    #[tokio::test]
    async fn noop_and_fallback_are_explicitly_distinct() {
        let method = Method::GET;
        let empty = RateLimitPlan::empty();
        NoopRateLimiter::new()
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &method,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &empty,
            })
            .await
            .expect("noop grants empty plan");
        FallbackRateLimiter
            .acquire(RateLimitRequest {
                endpoint: "RateApi::get",
                method: &method,
                safe_url: "https://example.test/get",
                host: Some("example.test"),
                page_index: 0,
                plan: &empty,
            })
            .await
            .expect("fallback grants empty plan");
    }
}
