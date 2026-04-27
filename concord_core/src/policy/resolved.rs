#![allow(dead_code)]

use crate::auth::AuthPlan;
use crate::cache::CacheSetting;
use crate::rate_limit::RateLimitPlan;
use crate::retry::RetrySetting;
use http::HeaderMap;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedPolicy {
    pub headers: HeaderMap,
    pub query: Vec<(String, String)>,
    pub timeout: Option<Duration>,
    pub auth: AuthPlan,
    pub cache: CacheSetting,
    pub retry: RetrySetting,
    pub rate_limit: RateLimitPlan,
}
