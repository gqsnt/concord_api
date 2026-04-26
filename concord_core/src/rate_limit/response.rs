use super::{RateLimitBucketId, RateLimitResponseContext};
use http::StatusCode;
use http::header::RETRY_AFTER;
use std::borrow::Cow;
use std::time::Duration;
use std::time::SystemTime;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RateLimitResponseAction {
    #[default]
    Continue,
    Limited {
        retry_after: Option<Duration>,
        target: RateLimitTarget,
        cooldown_stored: bool,
    },
}

impl RateLimitResponseAction {
    #[inline]
    pub fn is_limited(&self) -> bool {
        matches!(self, Self::Limited { .. })
    }

    #[inline]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Continue => None,
            Self::Limited { retry_after, .. } => *retry_after,
        }
    }

    #[inline]
    pub fn target(&self) -> Option<&RateLimitTarget> {
        match self {
            Self::Continue => None,
            Self::Limited { target, .. } => Some(target),
        }
    }

    #[inline]
    pub fn cooldown_stored(&self) -> bool {
        match self {
            Self::Continue => false,
            Self::Limited {
                cooldown_stored, ..
            } => *cooldown_stored,
        }
    }

    #[inline]
    pub fn delay_handled_by_rate_limiter(&self) -> bool {
        matches!(
            self,
            Self::Limited {
                retry_after: Some(_),
                cooldown_stored: true,
                ..
            }
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateLimitObservation {
    pub limited: bool,
    pub delay: Option<Duration>,
    pub target: RateLimitTarget,
    pub scope: Option<RateLimitScopeHint>,
    pub retry_after: Option<Duration>,
}

impl RateLimitObservation {
    #[inline]
    pub fn continue_() -> Self {
        Self::default()
    }

    #[inline]
    pub fn limited() -> Self {
        Self {
            limited: true,
            delay: None,
            target: RateLimitTarget::current_plan_or_endpoint(),
            scope: Some(RateLimitScopeHint::CurrentEndpoint),
            retry_after: None,
        }
    }

    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self.retry_after = Some(delay);
        self
    }

    #[inline]
    pub fn with_target(mut self, target: RateLimitTarget) -> Self {
        self.target = target;
        self
    }

    #[inline]
    pub fn scope_header(mut self, ctx: &RateLimitResponseContext<'_>, name: &str) -> Self {
        if let Some(value) = ctx.headers.get(name).and_then(|v| v.to_str().ok()) {
            let kind = value.trim();
            if !kind.is_empty() {
                self.scope = Some(RateLimitScopeHint::BucketKind(kind.to_string()));
                self.target = RateLimitTarget::bucket_kind(
                    kind.to_string(),
                    RateLimitTarget::current_plan_or_endpoint(),
                );
            }
        }
        self
    }

    #[inline]
    pub fn retry_after_header(mut self, ctx: &RateLimitResponseContext<'_>) -> Self {
        if let Some(delay) = parse_retry_after(ctx.headers) {
            self.delay = Some(delay);
            self.retry_after = Some(delay);
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RateLimitScopeHint {
    BucketKind(String),
    CurrentEndpoint,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RateLimitTarget {
    #[default]
    None,
    Request,
    Endpoint,
    Host,
    Client,
    CurrentPlan {
        fallback: Box<RateLimitTarget>,
    },
    BucketKind {
        kind: Cow<'static, str>,
        fallback: Box<RateLimitTarget>,
    },
    Bucket {
        id: RateLimitBucketId,
        fallback: Box<RateLimitTarget>,
    },
}

impl RateLimitTarget {
    #[inline]
    pub fn current_plan_or(fallback: RateLimitTarget) -> Self {
        Self::CurrentPlan {
            fallback: Box::new(fallback),
        }
    }

    #[inline]
    pub fn current_plan_or_endpoint() -> Self {
        Self::current_plan_or(Self::Endpoint)
    }

    #[inline]
    pub fn bucket_kind(kind: impl Into<Cow<'static, str>>, fallback: RateLimitTarget) -> Self {
        Self::BucketKind {
            kind: kind.into(),
            fallback: Box::new(fallback),
        }
    }

    #[inline]
    pub fn bucket(id: RateLimitBucketId, fallback: RateLimitTarget) -> Self {
        Self::Bucket {
            id,
            fallback: Box::new(fallback),
        }
    }
}

pub trait RateLimitResponsePolicy: Send + Sync + 'static {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation;
}

pub trait RateLimitObserver: Send + Sync + 'static {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation;
}

pub struct RateLimitObservationBuilder<'a> {
    ctx: &'a RateLimitResponseContext<'a>,
    observation: RateLimitObservation,
}

impl<'a> RateLimitObservationBuilder<'a> {
    #[inline]
    pub fn scope_header(mut self, name: &str) -> Self {
        self.observation = self.observation.scope_header(self.ctx, name);
        self
    }

    #[inline]
    pub fn retry_after(mut self) -> RateLimitObservation {
        self.observation = self.observation.retry_after_header(self.ctx);
        self.observation
    }

    #[inline]
    pub fn observe(self) -> RateLimitObservation {
        self.observation
    }
}

impl<'a> RateLimitResponseContext<'a> {
    #[inline]
    pub fn is_429(&self) -> bool {
        self.status == StatusCode::TOO_MANY_REQUESTS
    }

    #[inline]
    pub fn retry_after(&self) -> Option<Duration> {
        parse_retry_after(self.headers)
    }

    #[inline]
    pub fn on_429(&'a self) -> RateLimitObservationBuilder<'a> {
        let observation = if self.is_429() {
            RateLimitObservation::limited()
        } else {
            RateLimitObservation::continue_()
        };
        RateLimitObservationBuilder {
            ctx: self,
            observation,
        }
    }
}

impl<T> RateLimitResponsePolicy for T
where
    T: RateLimitObserver,
{
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        RateLimitObserver::observe(self, ctx.clone())
    }
}

#[derive(Default)]
pub struct DefaultRateLimitResponsePolicy;

impl RateLimitResponsePolicy for DefaultRateLimitResponsePolicy {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        if ctx.status != StatusCode::TOO_MANY_REQUESTS {
            return RateLimitObservation::continue_();
        }

        RateLimitObservation::limited()
            .with_target(RateLimitTarget::current_plan_or_endpoint())
            .retry_after_header(ctx)
    }
}

pub fn parse_retry_after(headers: &http::HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let when = httpdate::parse_http_date(raw).ok()?;
    match when.duration_since(SystemTime::now()) {
        Ok(delay) => Some(delay),
        Err(_) => Some(Duration::ZERO),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use http::HeaderValue;
    use std::time::Duration;
    use std::time::SystemTime;

    #[test]
    fn retry_after_parses_delta_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("5"));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(5)));
    }

    #[test]
    fn retry_after_parses_http_date() {
        let mut headers = HeaderMap::new();
        let when = SystemTime::now() + Duration::from_secs(3);
        let value = httpdate::fmt_http_date(when);
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_str(&value).expect("valid retry-after header value"),
        );
        let delay = parse_retry_after(&headers).expect("retry-after delay");
        assert!(delay <= Duration::from_secs(3));
    }
}
