use crate::error::ApiClientError;
use http::{HeaderMap, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;

type RateLimitFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct RateLimitContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub attempt: u32,
    pub page_index: u32,
    pub idempotent: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RateLimitPermit;

#[derive(Clone, Debug)]
pub struct RateLimitResponseContext<'a> {
    pub meta: RateLimitContext<'a>,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
}

pub trait RateLimiter: Send + Sync + 'static {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async { Ok(RateLimitPermit) })
    }

    fn on_response<'a>(&'a self, _ctx: RateLimitResponseContext<'a>) -> RateLimitFuture<'a, ()> {
        Box::pin(async {})
    }
}

#[derive(Default)]
pub struct NoopRateLimiter;

impl RateLimiter for NoopRateLimiter {}

