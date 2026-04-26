#![allow(dead_code)]

use crate::cache::CachePlan;
use crate::rate_limit::RateLimitPlan;
use crate::retry::RetryPlan;
use http::HeaderMap;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedPolicy {
    pub headers: HeaderMap,
    pub query: Vec<(String, String)>,
    pub timeout: Option<Duration>,
    pub cache: Option<CachePlan>,
    pub retry: Option<RetryPlan>,
    pub rate_limit: RateLimitPlan,
}
