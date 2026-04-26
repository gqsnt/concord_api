use crate::cache::{CacheStore, NoopCacheStore};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::inflight::{InflightPolicy, InflightRegistry, NoopInflightPolicy};
use crate::pagination::Caps;
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::retry::{NoRetryPolicy, RetryPolicy};
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthRuntimeConfig {
    pub max_retries: u32,
}

impl Default for AuthRuntimeConfig {
    fn default() -> Self {
        Self { max_retries: 8 }
    }
}

#[derive(Clone)]
pub struct DebugConfig {
    pub level: DebugLevel,
    pub sink: Arc<dyn DebugSink>,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            level: DebugLevel::default(),
            sink: Arc::new(StderrDebugSink),
        }
    }
}

#[derive(Clone)]
pub struct RuntimeConfig {
    pub hooks: Arc<dyn RuntimeHooks>,
    pub cache_store: Arc<dyn CacheStore>,
    pub inflight_policy: Arc<dyn InflightPolicy>,
    pub inflight_registry: Arc<InflightRegistry>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub retry_policy: Arc<dyn RetryPolicy>,
    pub auth: AuthRuntimeConfig,
    pub pagination: Caps,
    pub debug: DebugConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            hooks: Arc::new(NoopRuntimeHooks),
            cache_store: Arc::new(NoopCacheStore),
            inflight_policy: Arc::new(NoopInflightPolicy),
            inflight_registry: Arc::new(InflightRegistry::default()),
            rate_limiter: Arc::new(DefaultRateLimiter::default()),
            retry_policy: Arc::new(NoRetryPolicy),
            auth: AuthRuntimeConfig::default(),
            pagination: Caps::default(),
            debug: DebugConfig::default(),
        }
    }
}

impl RuntimeConfig {
    #[inline]
    pub fn debug(&mut self, level: DebugLevel) -> &mut Self {
        self.debug.level = level;
        self
    }

    #[inline]
    pub fn debug_sink(&mut self, sink: Arc<dyn DebugSink>) -> &mut Self {
        self.debug.sink = sink;
        self
    }

    #[inline]
    pub fn runtime_hooks(&mut self, hooks: Arc<dyn RuntimeHooks>) -> &mut Self {
        self.hooks = hooks;
        self
    }

    #[inline]
    pub fn cache_store(&mut self, store: Arc<dyn CacheStore>) -> &mut Self {
        self.cache_store = store;
        self
    }

    #[inline]
    pub fn inflight_policy(&mut self, policy: Arc<dyn InflightPolicy>) -> &mut Self {
        self.inflight_policy = policy;
        self
    }

    #[inline]
    pub fn rate_limiter(&mut self, limiter: Arc<dyn RateLimiter>) -> &mut Self {
        self.rate_limiter = limiter;
        self
    }

    #[inline]
    pub fn retry_policy(&mut self, policy: Arc<dyn RetryPolicy>) -> &mut Self {
        self.retry_policy = policy;
        self
    }

    #[inline]
    pub fn max_auth_retries(&mut self, max: u32) -> &mut Self {
        self.auth.max_retries = max;
        self
    }

    #[inline]
    pub fn pagination(&mut self, caps: Caps) -> &mut Self {
        self.pagination = caps;
        self
    }
}
