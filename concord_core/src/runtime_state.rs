use crate::rate_limit::RateLimiter;
use crate::retry::RetryPolicy;
use crate::runtime::RuntimeConfig;
use crate::runtime_hooks::RuntimeHooks;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct ClientRuntimeState {
    hooks: Arc<dyn RuntimeHooks>,
    rate_limiter: Arc<dyn RateLimiter>,
    retry_policy: Arc<dyn RetryPolicy>,
    max_auth_retries: u32,
    max_retry_delay: Duration,
    max_rate_limit_cooldown: Duration,
    max_response_body_bytes: Option<usize>,
    max_stream_request_body_bytes: Option<usize>,
    max_stream_response_body_bytes: Option<usize>,
    #[allow(deprecated)]
    dev_body_capture: Option<crate::runtime::DevBodyCaptureConfig>,
}

impl Default for ClientRuntimeState {
    fn default() -> Self {
        Self::from_config(RuntimeConfig::default())
    }
}

impl ClientRuntimeState {
    #[inline]
    pub fn from_config(config: RuntimeConfig) -> Self {
        Self {
            hooks: config.hooks,
            rate_limiter: config.rate_limiter,
            retry_policy: config.retry_policy,
            max_auth_retries: config.auth.max_retries,
            max_retry_delay: config.auth.max_retry_delay,
            max_rate_limit_cooldown: config.max_rate_limit_cooldown,
            max_response_body_bytes: config.max_response_body_bytes,
            max_stream_request_body_bytes: config.max_stream_request_body_bytes,
            max_stream_response_body_bytes: config.max_stream_response_body_bytes,
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
    pub fn max_retry_delay(&self) -> Duration {
        self.max_retry_delay
    }

    #[inline]
    pub fn set_max_retry_delay(&mut self, max_retry_delay: Duration) {
        self.max_retry_delay = max_retry_delay;
    }

    #[inline]
    pub fn max_rate_limit_cooldown(&self) -> Duration {
        self.max_rate_limit_cooldown
    }

    #[inline]
    pub fn set_max_rate_limit_cooldown(&mut self, max_rate_limit_cooldown: Duration) {
        self.max_rate_limit_cooldown = max_rate_limit_cooldown;
    }

    #[inline]
    pub fn max_response_body_bytes(&self) -> Option<usize> {
        self.max_response_body_bytes
    }

    #[inline]
    pub fn max_stream_request_body_bytes(&self) -> Option<usize> {
        self.max_stream_request_body_bytes
    }

    #[inline]
    pub fn max_stream_response_body_bytes(&self) -> Option<usize> {
        self.max_stream_response_body_bytes
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
