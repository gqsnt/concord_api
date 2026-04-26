use crate::cache::{CacheConfig, CacheSetting};
use crate::error::{ApiClientError, ErrorContext};
use crate::rate_limit::RateLimitPlan;
use crate::retry::{RetryConfig, RetrySetting};
use core::time::Duration;
use http::header::{ACCEPT, CONTENT_TYPE, HeaderName};
use http::{HeaderMap, HeaderValue};

pub mod feature;
pub mod resolved;
#[allow(unused_imports)]
pub use feature::FeatureUse;
#[allow(unused_imports)]
pub use resolved::ResolvedPolicy;

pub type PolicyParts = (
    HeaderMap,
    Vec<(String, String)>,
    Option<Duration>,
    CacheSetting,
    RetrySetting,
    RateLimitPlan,
);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
#[derive(Default)]
pub enum PolicyLayer {
    #[default]
    Client = 0,
    /// Reserved for future “outer → inner prefix/path” policy layers.
    PrefixPath = 1,
    Endpoint = 2,
    Runtime = 3,
}

#[derive(Default)]
pub struct Policy {
    headers: HeaderMap,
    query: Vec<(String, String)>,
    timeout: Option<Duration>,
    cache: CacheSetting,
    retry: RetrySetting,
    rate_limit: RateLimitPlan,
    // Current layer used for provenance decisions (not exposed in into_parts()).
    layer: PolicyLayer,

    // If endpoint policy explicitly sets OR removes Accept, runtime decoder injection must not override it.
    accept_explicit_by_endpoint: bool,
    accept_explicit_by_runtime: bool,
}

impl Policy {
    pub fn new() -> Self {
        Self {
            headers: HeaderMap::new(),
            query: Vec::new(),
            timeout: None,
            cache: CacheSetting::Inherit,
            retry: RetrySetting::Inherit,
            rate_limit: RateLimitPlan::new(),
            layer: PolicyLayer::Client,
            accept_explicit_by_endpoint: false,
            accept_explicit_by_runtime: false,
        }
    }

    #[inline]
    pub fn layer(&self) -> PolicyLayer {
        self.layer
    }

    #[inline]
    pub fn set_layer(&mut self, layer: PolicyLayer) {
        self.layer = layer;
    }

    #[inline]
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    #[inline]
    pub fn set_timeout(&mut self, d: Duration) {
        self.timeout = Some(d);
    }

    #[inline]
    pub fn clear_timeout(&mut self) {
        self.timeout = None;
    }

    #[inline]
    pub fn cache(&self) -> Option<&CacheConfig> {
        match &self.cache {
            CacheSetting::Config(config) => Some(config),
            CacheSetting::Inherit | CacheSetting::Off => None,
        }
    }

    #[inline]
    pub fn set_cache(&mut self, cache: CacheConfig) {
        self.cache = CacheSetting::Config(cache);
    }

    #[inline]
    pub fn clear_cache(&mut self) {
        self.cache = CacheSetting::Off;
    }

    #[inline]
    pub fn retry(&self) -> Option<&RetryConfig> {
        match &self.retry {
            RetrySetting::Config(config) => Some(config),
            RetrySetting::Inherit | RetrySetting::Off => None,
        }
    }

    #[inline]
    pub fn set_retry(&mut self, retry: RetryConfig) {
        self.retry = RetrySetting::Config(retry);
    }

    #[inline]
    pub fn clear_retry(&mut self) {
        self.retry = RetrySetting::Off;
    }

    #[inline]
    pub fn rate_limit(&self) -> &RateLimitPlan {
        &self.rate_limit
    }

    #[inline]
    pub fn add_rate_limit(&mut self, plan: RateLimitPlan) {
        self.rate_limit.extend(plan);
    }

    #[inline]
    pub fn replace_rate_limit(&mut self, plan: RateLimitPlan) {
        self.rate_limit = plan;
    }

    #[inline]
    pub fn clear_rate_limit(&mut self) {
        self.rate_limit = RateLimitPlan::new();
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn query(&self) -> &[(String, String)] {
        &self.query
    }

    pub fn insert_header(&mut self, name: HeaderName, value: HeaderValue) {
        if self.layer == PolicyLayer::Endpoint && name == ACCEPT {
            self.accept_explicit_by_endpoint = true;
        }
        self.headers.insert(name, value);
    }

    pub fn remove_header(&mut self, name: HeaderName) {
        if self.layer == PolicyLayer::Endpoint && name == ACCEPT {
            self.accept_explicit_by_endpoint = true;
        }
        let _ = self.headers.remove(name);
    }

    pub fn has_content_type(&self) -> bool {
        self.headers.contains_key(CONTENT_TYPE)
    }

    /// Decoder-driven Accept injection:
    /// - Applied at runtime (after endpoint policy).
    /// - Overrides base/prefix/path Accept.
    /// - Does NOT override if endpoint policy explicitly set OR removed Accept.
    pub fn ensure_accept(&mut self, ct: &'static str) {
        if ct.is_empty() {
            return;
        }
        if self.accept_explicit_by_endpoint || self.accept_explicit_by_runtime {
            return;
        }
        // Always override whatever was there (base/prefix/path), because decoder owns Accept.
        self.headers.insert(ACCEPT, HeaderValue::from_static(ct));
    }

    // ---------------- Query helpers ----------------

    /// Append (allow duplicates): current behavior.
    pub fn push_query(&mut self, key: &str, value: impl Into<String>) {
        self.query.push((key.to_string(), value.into()));
    }

    /// Override-by-key: remove existing entries with same key, then insert.
    pub fn set_query(&mut self, key: &str, value: impl Into<String>) {
        self.remove_query(key);
        self.query.push((key.to_string(), value.into()));
    }

    /// Remove all entries matching `key`.
    pub fn remove_query(&mut self, key: &str) {
        self.query.retain(|(k, _)| k != key);
    }

    pub fn into_parts(self) -> PolicyParts {
        (
            self.headers,
            self.query,
            self.timeout,
            self.cache,
            self.retry,
            self.rate_limit,
        )
    }
}

pub struct PolicyPatch<'a> {
    ctx: ErrorContext,
    inner: &'a mut Policy,
}

impl<'a> PolicyPatch<'a> {
    #[inline]
    pub(crate) fn new(ctx: ErrorContext, inner: &'a mut Policy) -> Self {
        Self { ctx, inner }
    }

    #[inline]
    pub fn set_header(
        &mut self,
        name: HeaderName,
        value: HeaderValue,
    ) -> Result<(), ApiClientError> {
        self.guard_accept(&name)?;
        self.inner.insert_header(name, value);
        Ok(())
    }

    #[inline]
    pub fn remove_header(&mut self, name: HeaderName) -> Result<(), ApiClientError> {
        self.guard_accept(&name)?;
        self.inner.remove_header(name);
        Ok(())
    }

    #[inline]
    pub fn push_query(&mut self, key: &str, value: impl Into<String>) {
        self.inner.push_query(key, value);
    }

    #[inline]
    pub fn set_query(&mut self, key: &str, value: impl Into<String>) {
        self.inner.set_query(key, value);
    }

    #[inline]
    pub fn remove_query(&mut self, key: &str) {
        self.inner.remove_query(key);
    }

    #[inline]
    pub fn set_timeout_override(&mut self, t: Option<Duration>) {
        self.inner.timeout = t;
    }

    #[inline]
    pub fn set_cache_override(&mut self, cache: Option<CacheConfig>) {
        self.inner.cache = cache.map_or(CacheSetting::Off, CacheSetting::Config);
    }

    #[inline]
    pub fn set_retry_override(&mut self, retry: Option<RetryConfig>) {
        self.inner.retry = retry.map_or(RetrySetting::Off, RetrySetting::Config);
    }

    #[inline]
    pub fn add_rate_limit(&mut self, plan: RateLimitPlan) {
        self.inner.add_rate_limit(plan);
    }

    #[inline]
    pub fn replace_rate_limit(&mut self, plan: RateLimitPlan) {
        self.inner.replace_rate_limit(plan);
    }

    #[inline]
    pub fn clear_rate_limit(&mut self) {
        self.inner.clear_rate_limit();
    }

    #[inline]
    pub fn set_accept_override(&mut self, v: Option<HeaderValue>) {
        // Explicit override authorizes runtime changes coherently.
        self.inner.accept_explicit_by_runtime = true;
        match v {
            Some(hv) => {
                self.inner.headers.insert(ACCEPT, hv);
            }
            None => {
                let _ = self.inner.headers.remove(ACCEPT);
            }
        }
    }

    fn guard_accept(&self, name: &HeaderName) -> Result<(), ApiClientError> {
        if self.inner.layer == PolicyLayer::Runtime
            && *name == ACCEPT
            && !(self.inner.accept_explicit_by_endpoint || self.inner.accept_explicit_by_runtime)
        {
            return Err(ApiClientError::PolicyViolation {
                ctx: self.ctx.clone(),
                msg: "runtime cannot override Accept unless endpoint explicitly set/removed it",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::header::ACCEPT;

    #[test]
    fn runtime_accept_overrides_base_unless_endpoint_explicit() {
        // base Accept gets overridden by runtime decoder Accept
        let mut p = Policy::new();
        p.insert_header(ACCEPT, HeaderValue::from_static("text/plain"));
        p.set_layer(PolicyLayer::Runtime);
        p.ensure_accept("application/json");
        assert_eq!(
            p.headers().get(ACCEPT).unwrap().to_str().unwrap(),
            "application/json"
        );

        // endpoint explicit Accept prevents override
        let mut p = Policy::new();
        p.insert_header(ACCEPT, HeaderValue::from_static("text/plain"));
        p.set_layer(PolicyLayer::Endpoint);
        p.insert_header(ACCEPT, HeaderValue::from_static("application/custom"));
        p.set_layer(PolicyLayer::Runtime);
        p.ensure_accept("application/json");
        assert_eq!(
            p.headers().get(ACCEPT).unwrap().to_str().unwrap(),
            "application/custom"
        );

        // endpoint explicit removal prevents runtime injection
        let mut p = Policy::new();
        p.insert_header(ACCEPT, HeaderValue::from_static("text/plain"));
        p.set_layer(PolicyLayer::Endpoint);
        p.remove_header(ACCEPT);
        p.set_layer(PolicyLayer::Runtime);
        p.ensure_accept("application/json");
        assert!(p.headers().get(ACCEPT).is_none());
    }
}
