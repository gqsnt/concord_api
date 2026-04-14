use super::{RateLimitBucketId, RateLimitResponseContext};
use http::StatusCode;
use http::header::RETRY_AFTER;
use std::borrow::Cow;
use std::time::Duration;

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
        }
    }

    #[inline]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    #[inline]
    pub fn with_target(mut self, target: RateLimitTarget) -> Self {
        self.target = target;
        self
    }
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

#[derive(Default)]
pub struct DefaultRateLimitResponsePolicy;

impl RateLimitResponsePolicy for DefaultRateLimitResponsePolicy {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        if ctx.status != StatusCode::TOO_MANY_REQUESTS {
            return RateLimitObservation::continue_();
        }

        let mut observation = RateLimitObservation::limited()
            .with_target(RateLimitTarget::current_plan_or_endpoint());
        if let Some(delay) = parse_retry_after(ctx.headers) {
            observation = observation.with_delay(delay);
        }
        observation
    }
}

pub fn parse_retry_after(headers: &http::HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds = raw.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}
