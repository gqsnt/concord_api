use super::limiter::{RateLimitFuture, RateLimiter};
use super::{RateLimitContext, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext};
use crate::error::{ApiClientError, ErrorContext};
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct DefaultRateLimiter;

#[derive(Default, Clone)]
pub struct GovernorRateLimiter;

impl DefaultRateLimiter {
    pub const DEFAULT_MAX_COOLDOWN_ENTRIES: usize = 4096;

    #[inline]
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn with_response_policy(self, _policy: Arc<dyn super::RateLimitResponsePolicy>) -> Self {
        self
    }

    #[inline]
    pub fn with_max_cooldown_entries(self, _max_cooldown_entries: usize) -> Self {
        self
    }
}

impl GovernorRateLimiter {
    pub const DEFAULT_MAX_COOLDOWN_ENTRIES: usize =
        DefaultRateLimiter::DEFAULT_MAX_COOLDOWN_ENTRIES;

    #[inline]
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn with_response_policy(self, _policy: Arc<dyn super::RateLimitResponsePolicy>) -> Self {
        Self
    }

    #[inline]
    pub fn with_max_cooldown_entries(self, _max_cooldown_entries: usize) -> Self {
        self
    }
}

impl RateLimiter for DefaultRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        if ctx.plan.is_empty() {
            return Box::pin(async { Ok(RateLimitPermit) });
        }

        Box::pin(async move {
            Err(rate_limit_disabled_error(
                &ctx,
                "rate-limit-governor feature is disabled; non-empty rate-limit plans require an explicit opt-out",
            ))
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        if ctx.meta.plan.is_empty() {
            return Box::pin(async { Ok(RateLimitResponseAction::Continue) });
        }

        Box::pin(async move {
            Err(rate_limit_disabled_error(
                &ctx.meta,
                "rate-limit-governor feature is disabled; non-empty rate-limit plans require an explicit opt-out",
            ))
        })
    }
}

impl RateLimiter for GovernorRateLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        if ctx.plan.is_empty() {
            return Box::pin(async { Ok(RateLimitPermit) });
        }

        Box::pin(async move {
            Err(rate_limit_disabled_error(
                &ctx,
                "rate-limit-governor feature is disabled; non-empty rate-limit plans require an explicit opt-out",
            ))
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        if ctx.meta.plan.is_empty() {
            return Box::pin(async { Ok(RateLimitResponseAction::Continue) });
        }

        Box::pin(async move {
            Err(rate_limit_disabled_error(
                &ctx.meta,
                "rate-limit-governor feature is disabled; non-empty rate-limit plans require an explicit opt-out",
            ))
        })
    }
}

fn rate_limit_disabled_error(ctx: &RateLimitContext<'_>, msg: &'static str) -> ApiClientError {
    ApiClientError::rate_limit(
        ErrorContext {
            endpoint: ctx.endpoint,
            method: ctx.method.clone(),
        },
        crate::rate_limit::RateLimitErrorKind::InvalidConfiguration,
        msg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::{
        RateLimitBucketUse, RateLimitKey, RateLimitKeyPart, RateLimitPlan, RateLimitWindow,
    };
    use http::Method;
    use std::num::NonZeroU32;
    use std::time::Duration;
    use tokio::runtime::Builder;

    fn empty_ctx() -> RateLimitContext<'static> {
        static METHOD: Method = Method::GET;
        static URL: &str = "https://example.com/empty";
        static ENDPOINT: &str = "Empty";
        let plan = Box::leak(Box::new(RateLimitPlan::default()));
        RateLimitContext {
            endpoint: ENDPOINT,
            method: &METHOD,
            url: URL,
            url_host: Some("example.com"),
            attempt: 0,
            page_index: 0,
            idempotent: true,
            max_cooldown: Duration::from_secs(60),
            plan,
        }
    }

    fn non_empty_ctx() -> RateLimitContext<'static> {
        static METHOD: Method = Method::GET;
        static URL: &str = "https://example.com/non-empty";
        static ENDPOINT: &str = "NonEmpty";
        let bucket = RateLimitBucketUse::new(
            "method",
            "test",
            RateLimitKey::new(vec![RateLimitKeyPart::static_value("k", "v")]),
        )
        .with_windows(vec![RateLimitWindow::new(
            NonZeroU32::new(10).expect("non-zero"),
            Duration::from_secs(10),
        )]);
        let plan = Box::leak(Box::new(RateLimitPlan::from_buckets(vec![bucket])));
        RateLimitContext {
            endpoint: ENDPOINT,
            method: &METHOD,
            url: URL,
            url_host: Some("example.com"),
            attempt: 0,
            page_index: 0,
            idempotent: true,
            max_cooldown: Duration::from_secs(60),
            plan,
        }
    }

    #[test]
    fn default_limiter_allows_empty_plans() {
        let limiter = DefaultRateLimiter::default();
        let permit = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(limiter.acquire(empty_ctx()))
            .expect("empty plans should be allowed");
        let _ = permit;
    }

    #[test]
    fn default_limiter_fails_closed_for_non_empty_plans() {
        let limiter = DefaultRateLimiter::default();
        let err = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(limiter.acquire(non_empty_ctx()))
            .expect_err("non-empty plans should fail closed");
        assert_eq!(err.category(), crate::error::ErrorCategory::RateLimit);
        assert_eq!(
            err.rate_limit_error().map(|err| err.kind()),
            Some(crate::rate_limit::RateLimitErrorKind::InvalidConfiguration)
        );
        assert!(err.to_string().contains("explicit opt-out"));
    }

    #[test]
    fn noop_rate_limiter_remains_an_explicit_opt_out() {
        let limiter = crate::rate_limit::NoopRateLimiter::new();
        Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(limiter.acquire(non_empty_ctx()))
            .expect("explicit noop limiter should still opt out");
    }
}
