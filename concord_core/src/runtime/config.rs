use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct DebugConfig {
    pub(crate) level: DebugLevel,
    pub(crate) sink: Arc<dyn DebugSink>,
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
    pub(crate) hooks: Arc<dyn RuntimeHooks>,
    pub(crate) rate_limiter: Arc<dyn RateLimiter>,
    pub(crate) max_rate_limit_cooldown: Duration,
    pub(crate) pagination_detect_loops: bool,
    pub(crate) debug: DebugConfig,
    pub(crate) max_response_body_bytes: Option<usize>,
    pub(crate) max_request_body_bytes: Option<usize>,
    pub(crate) max_stream_response_body_bytes: Option<usize>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            hooks: Arc::new(NoopRuntimeHooks),
            rate_limiter: Arc::new(DefaultRateLimiter::new()),
            max_rate_limit_cooldown: Duration::from_secs(60),
            pagination_detect_loops: true,
            debug: DebugConfig::default(),
            max_response_body_bytes: Some(16 * 1024 * 1024),
            max_request_body_bytes: Some(16 * 1024 * 1024),
            max_stream_response_body_bytes: Some(16 * 1024 * 1024),
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
    pub fn rate_limiter(&mut self, limiter: Arc<dyn RateLimiter>) -> &mut Self {
        self.rate_limiter = limiter;
        self
    }

    #[inline]
    pub fn max_rate_limit_cooldown(&mut self, max_delay: Duration) -> &mut Self {
        self.max_rate_limit_cooldown = max_delay;
        self
    }

    #[inline]
    pub fn pagination_detect_loops(&mut self, enabled: bool) -> &mut Self {
        self.pagination_detect_loops = enabled;
        self
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

    /// Sets the request-body limit for reusable bytes, encoded JSON/text,
    /// streams, advanced bodies, factory results, and multipart bridge output.
    /// Exact/inherent oversize may fail early; advisory upper hints do not.
    #[inline]
    pub fn max_request_body_bytes(&mut self, bytes: usize) -> &mut Self {
        self.max_request_body_bytes = Some(bytes);
        self
    }

    /// Disables the all-request-body limit.
    #[inline]
    pub fn no_request_body_limit(&mut self) -> &mut Self {
        self.max_request_body_bytes = None;
        self
    }

    #[inline]
    pub fn max_stream_response_body_bytes(&mut self, bytes: usize) -> &mut Self {
        self.max_stream_response_body_bytes = Some(bytes);
        self
    }

    #[inline]
    pub fn no_stream_response_body_limit(&mut self) -> &mut Self {
        self.max_stream_response_body_bytes = None;
        self
    }

    #[inline]
    pub fn debug_level_value(&self) -> DebugLevel {
        self.debug.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_defaults_snapshot() {
        let cfg = RuntimeConfig::default();

        assert!(cfg.pagination_detect_loops);
        assert_eq!(cfg.debug.level, DebugLevel::None);
        assert_eq!(cfg.max_response_body_bytes, Some(16 * 1024 * 1024));
        assert_eq!(cfg.max_request_body_bytes, Some(16 * 1024 * 1024));
        assert_eq!(cfg.max_stream_response_body_bytes, Some(16 * 1024 * 1024));
        assert_eq!(cfg.max_rate_limit_cooldown, Duration::from_secs(60));
        assert_eq!(Arc::strong_count(&cfg.hooks), 1);
        assert_eq!(Arc::strong_count(&cfg.rate_limiter), 1);
        assert_eq!(Arc::strong_count(&cfg.debug.sink), 1);
    }
}
