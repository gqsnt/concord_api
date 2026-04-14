use super::{
    RateLimitContext, RateLimitPermit, RateLimitResponseAction, RateLimitResponseContext,
    RateLimitResponsePolicy,
};
use crate::error::ApiClientError;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type RateLimitFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait RateLimiter: Send + Sync + 'static {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async { Ok(RateLimitPermit) })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        Box::pin(async { Ok(RateLimitResponseAction::Continue) })
    }
}

#[derive(Default)]
pub struct NoopRateLimiter;

impl NoopRateLimiter {
    #[inline]
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn with_response_policy(self, _policy: Arc<dyn RateLimitResponsePolicy>) -> Self {
        self
    }
}

impl RateLimiter for NoopRateLimiter {}
