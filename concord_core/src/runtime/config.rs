use crate::cache::{CacheStore, NoopCacheStore};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::pagination::Caps;
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::retry::{NoRetryPolicy, RetryPolicy};
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthRuntimeConfig {
    pub(crate) max_retries: u32,
}

impl Default for AuthRuntimeConfig {
    fn default() -> Self {
        Self { max_retries: 8 }
    }
}

#[derive(Clone)]
pub struct DebugConfig {
    pub(crate) level: DebugLevel,
    pub(crate) sink: Arc<dyn DebugSink>,
    pub(crate) body: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            level: DebugLevel::default(),
            sink: Arc::new(StderrDebugSink),
            body: false,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeConfig {
    pub(crate) hooks: Arc<dyn RuntimeHooks>,
    pub(crate) cache_store: Arc<dyn CacheStore>,
    pub(crate) rate_limiter: Arc<dyn RateLimiter>,
    pub(crate) retry_policy: Arc<dyn RetryPolicy>,
    pub(crate) auth: AuthRuntimeConfig,
    pub(crate) pagination: Caps,
    pub(crate) debug: DebugConfig,
    pub(crate) max_response_body_bytes: Option<usize>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            hooks: Arc::new(NoopRuntimeHooks),
            cache_store: Arc::new(NoopCacheStore),
            rate_limiter: Arc::new(DefaultRateLimiter::default()),
            retry_policy: Arc::new(NoRetryPolicy),
            auth: AuthRuntimeConfig::default(),
            pagination: Caps::default(),
            debug: DebugConfig::default(),
            max_response_body_bytes: Some(16 * 1024 * 1024),
        }
    }
}

impl RuntimeConfig {
    #[inline]
    pub fn debug_level(&mut self, level: DebugLevel) -> &mut Self {
        self.debug.level = level;
        self
    }

    #[inline]
    pub fn debug_body(&mut self, enabled: bool) -> &mut Self {
        self.debug.body = enabled;
        self
    }

    #[inline]
    pub fn debug(&mut self, level: DebugLevel) -> &mut Self {
        self.debug_level(level)
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
    pub fn pagination_caps(&mut self, caps: Caps) -> &mut Self {
        self.pagination = caps;
        self
    }

    #[inline]
    pub fn pagination(&mut self, caps: Caps) -> &mut Self {
        self.pagination_caps(caps)
    }

    #[inline]
    pub fn max_response_body_bytes(&mut self, bytes: usize) -> &mut Self {
        self.max_response_body_bytes = Some(bytes);
        self
    }

    #[inline]
    pub fn no_response_body_limit(&mut self) -> &mut Self {
        self.max_response_body_bytes = None;
        self
    }

    #[inline]
    pub fn debug_body_enabled(&self) -> bool {
        self.debug.body
    }

    #[inline]
    pub fn debug_level_value(&self) -> DebugLevel {
        self.debug.level
    }
}
