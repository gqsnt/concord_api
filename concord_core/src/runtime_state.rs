use crate::cache::{CacheStore, NoopCacheStore};
use crate::inflight::{InflightPolicy, InflightRegistry, NoopInflightPolicy};
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::retry::{NoRetryPolicy, RetryPolicy};
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use std::sync::Arc;

#[derive(Clone)]
pub struct ClientRuntimeState {
    hooks: Arc<dyn RuntimeHooks>,
    cache_store: Arc<dyn CacheStore>,
    inflight_policy: Arc<dyn InflightPolicy>,
    inflight_registry: Arc<InflightRegistry>,
    rate_limiter: Arc<dyn RateLimiter>,
    retry_policy: Arc<dyn RetryPolicy>,
    max_auth_retries: u32,
}

impl Default for ClientRuntimeState {
    fn default() -> Self {
        Self {
            hooks: Arc::new(NoopRuntimeHooks),
            cache_store: Arc::new(NoopCacheStore),
            inflight_policy: Arc::new(NoopInflightPolicy),
            inflight_registry: Arc::new(InflightRegistry::default()),
            rate_limiter: Arc::new(DefaultRateLimiter::default()),
            retry_policy: Arc::new(NoRetryPolicy),
            max_auth_retries: 8,
        }
    }
}

impl ClientRuntimeState {
    #[inline]
    pub fn hooks(&self) -> &Arc<dyn RuntimeHooks> {
        &self.hooks
    }

    #[inline]
    pub fn set_hooks(&mut self, hooks: Arc<dyn RuntimeHooks>) {
        self.hooks = hooks;
    }

    #[inline]
    pub fn cache_store(&self) -> &Arc<dyn CacheStore> {
        &self.cache_store
    }

    #[inline]
    pub fn set_cache_store(&mut self, cache_store: Arc<dyn CacheStore>) {
        self.cache_store = cache_store;
    }

    #[inline]
    pub fn retry_policy(&self) -> &Arc<dyn RetryPolicy> {
        &self.retry_policy
    }

    #[inline]
    pub fn set_retry_policy(&mut self, retry_policy: Arc<dyn RetryPolicy>) {
        self.retry_policy = retry_policy;
    }

    #[inline]
    pub fn max_auth_retries(&self) -> u32 {
        self.max_auth_retries
    }

    #[inline]
    pub fn set_max_auth_retries(&mut self, max_auth_retries: u32) {
        self.max_auth_retries = max_auth_retries;
    }

    #[inline]
    pub fn inflight_policy(&self) -> &Arc<dyn InflightPolicy> {
        &self.inflight_policy
    }

    #[inline]
    pub fn set_inflight_policy(&mut self, inflight_policy: Arc<dyn InflightPolicy>) {
        self.inflight_policy = inflight_policy;
    }

    #[inline]
    pub fn inflight_registry(&self) -> &Arc<InflightRegistry> {
        &self.inflight_registry
    }

    #[inline]
    pub fn rate_limiter(&self) -> &Arc<dyn RateLimiter> {
        &self.rate_limiter
    }

    #[inline]
    pub fn set_rate_limiter(&mut self, rate_limiter: Arc<dyn RateLimiter>) {
        self.rate_limiter = rate_limiter;
    }
}
