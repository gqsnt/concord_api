use crate::cache::{CacheStore, NoopCacheStore};
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::retry::{NoRetryPolicy, RetryPolicy};
use crate::runtime::RuntimeConfig;
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use std::sync::Arc;

#[derive(Clone)]
pub struct ClientRuntimeState {
    hooks: Arc<dyn RuntimeHooks>,
    cache_store: Arc<dyn CacheStore>,
    rate_limiter: Arc<dyn RateLimiter>,
    retry_policy: Arc<dyn RetryPolicy>,
    max_auth_retries: u32,
    max_response_body_bytes: Option<usize>,
    #[allow(deprecated)]
    dev_body_capture: Option<crate::runtime::DevBodyCaptureConfig>,
}

impl Default for ClientRuntimeState {
    fn default() -> Self {
        Self {
            hooks: Arc::new(NoopRuntimeHooks),
            cache_store: Arc::new(NoopCacheStore),
            rate_limiter: Arc::new(DefaultRateLimiter::default()),
            retry_policy: Arc::new(NoRetryPolicy),
            max_auth_retries: 8,
            max_response_body_bytes: Some(16 * 1024 * 1024),
            dev_body_capture: None,
        }
    }
}

impl ClientRuntimeState {
    #[inline]
    pub fn from_config(config: RuntimeConfig) -> Self {
        Self {
            hooks: config.hooks,
            cache_store: config.cache_store,
            rate_limiter: config.rate_limiter,
            retry_policy: config.retry_policy,
            max_auth_retries: config.auth.max_retries,
            max_response_body_bytes: config.max_response_body_bytes,
            dev_body_capture: config.dev_body_capture,
        }
    }

    #[inline]
    pub fn apply_config(&mut self, config: RuntimeConfig) {
        *self = Self::from_config(config);
    }

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
    pub fn max_response_body_bytes(&self) -> Option<usize> {
        self.max_response_body_bytes
    }

    #[allow(deprecated)]
    #[inline]
    pub fn dev_body_capture(&self) -> Option<&crate::runtime::DevBodyCaptureConfig> {
        self.dev_body_capture.as_ref()
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

impl From<RuntimeConfig> for ClientRuntimeState {
    fn from(value: RuntimeConfig) -> Self {
        Self::from_config(value)
    }
}
