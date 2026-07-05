use super::RateLimitPlan;
use http::{HeaderMap, Method, StatusCode};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct RateLimitContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub url_host: Option<&'a str>,
    pub attempt: u32,
    pub page_index: u32,
    pub idempotent: bool,
    pub max_cooldown: Duration,
    pub plan: &'a RateLimitPlan,
}

#[derive(Clone, Debug, Default)]
pub struct RateLimitPermit;

#[derive(Clone, Debug)]
pub struct RateLimitResponseContext<'a> {
    pub meta: RateLimitContext<'a>,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
    pub max_cooldown: Duration,
}
