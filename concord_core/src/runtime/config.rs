use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::retry::{NoRetryPolicy, RetryPolicy};
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "dangerous-dev-tools")]
use bytes::Bytes;
#[cfg(feature = "dangerous-dev-tools")]
use http::{Method, StatusCode};
#[cfg(feature = "dangerous-dev-tools")]
use std::fs::{self, OpenOptions};
#[cfg(feature = "dangerous-dev-tools")]
use std::io::Write;
#[cfg(feature = "dangerous-dev-tools")]
use std::path::{Path, PathBuf};
#[cfg(feature = "dangerous-dev-tools")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "dangerous-dev-tools")]
static DEV_BODY_CAPTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

#[cfg(feature = "dangerous-dev-tools")]
#[deprecated(
    note = "dev-only diagnostic body capture; disabled by default; may persist sensitive response bytes to local disk; do not use in production"
)]
#[derive(Clone, Debug)]
pub struct DevBodyCaptureConfig {
    pub(crate) dir: PathBuf,
    pub(crate) max_bytes: usize,
}

#[cfg(feature = "dangerous-dev-tools")]
#[allow(deprecated)]
impl DevBodyCaptureConfig {
    pub const DEFAULT_MAX_BYTES: usize = 64 * 1024;

    #[inline]
    pub fn response_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            max_bytes: Self::DEFAULT_MAX_BYTES,
        }
    }

    #[inline]
    pub fn max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    pub(crate) fn capture_response(
        &self,
        endpoint: &'static str,
        method: &Method,
        status: StatusCode,
        body: &Bytes,
    ) {
        let _ = self.try_capture_response(endpoint, method, status, body);
    }

    fn try_capture_response(
        &self,
        endpoint: &'static str,
        method: &Method,
        status: StatusCode,
        body: &Bytes,
    ) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let counter = DEV_BODY_CAPTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!(
            "{}-{}-{}-{counter}.body",
            sanitize_path_component(endpoint),
            sanitize_path_component(method.as_str()),
            status.as_u16()
        );
        if body.len() > self.max_bytes {
            return Ok(());
        }
        let path = safe_capture_path(&self.dir, &filename);
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(body)?;
        Ok(())
    }
}

#[cfg(feature = "dangerous-dev-tools")]
fn sanitize_path_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len().max(1));
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}

#[cfg(feature = "dangerous-dev-tools")]
fn safe_capture_path(dir: &Path, filename: &str) -> PathBuf {
    debug_assert!(!filename.contains('/') && !filename.contains('\\'));
    dir.join(filename)
}

#[derive(Clone)]
pub struct RuntimeConfig {
    pub(crate) hooks: Arc<dyn RuntimeHooks>,
    pub(crate) rate_limiter: Arc<dyn RateLimiter>,
    pub(crate) retry_policy: Arc<dyn RetryPolicy>,
    pub(crate) max_attempts: u32,
    pub(crate) respect_retry_after: bool,
    pub(crate) max_rate_limit_cooldown: Duration,
    pub(crate) pagination_detect_loops: bool,
    pub(crate) debug: DebugConfig,
    pub(crate) max_response_body_bytes: Option<usize>,
    pub(crate) max_stream_request_body_bytes: Option<usize>,
    pub(crate) max_stream_response_body_bytes: Option<usize>,
    #[cfg(feature = "dangerous-dev-tools")]
    #[allow(deprecated)]
    pub(crate) dev_body_capture: Option<DevBodyCaptureConfig>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            hooks: Arc::new(NoopRuntimeHooks),
            rate_limiter: Arc::new(DefaultRateLimiter::default()),
            retry_policy: Arc::new(NoRetryPolicy),
            max_attempts: 1,
            respect_retry_after: false,
            max_rate_limit_cooldown: Duration::from_secs(60),
            pagination_detect_loops: true,
            debug: DebugConfig::default(),
            max_response_body_bytes: Some(16 * 1024 * 1024),
            max_stream_request_body_bytes: Some(16 * 1024 * 1024),
            max_stream_response_body_bytes: Some(16 * 1024 * 1024),
            #[cfg(feature = "dangerous-dev-tools")]
            dev_body_capture: None,
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

    #[cfg(feature = "dangerous-dev-tools")]
    #[deprecated(
        note = "dev-only diagnostic body capture; disabled by default; may persist sensitive response bytes to local disk; do not use in production"
    )]
    #[allow(deprecated)]
    #[inline]
    pub fn dev_body_capture(&mut self, capture: DevBodyCaptureConfig) -> &mut Self {
        self.dev_body_capture = Some(capture);
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
    pub fn retry_policy(&mut self, policy: Arc<dyn RetryPolicy>) -> &mut Self {
        self.retry_policy = policy;
        self
    }

    /// Sets the absolute cap used when an endpoint inherits the runtime
    /// classifier. Invalid values are rejected when that configuration is
    /// applied to a request; the value is never clamped.
    #[inline]
    pub fn max_attempts(&mut self, max_attempts: u32) -> &mut Self {
        self.max_attempts = max_attempts;
        self
    }

    #[inline]
    pub fn respect_retry_after(&mut self, enabled: bool) -> &mut Self {
        self.respect_retry_after = enabled;
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

    #[inline]
    pub fn max_stream_request_body_bytes(&mut self, bytes: usize) -> &mut Self {
        self.max_stream_request_body_bytes = Some(bytes);
        self
    }

    #[inline]
    pub fn no_stream_request_body_limit(&mut self) -> &mut Self {
        self.max_stream_request_body_bytes = None;
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

        assert_eq!(cfg.max_attempts, 1);
        assert!(!cfg.respect_retry_after);
        assert!(cfg.pagination_detect_loops);
        assert_eq!(cfg.debug.level, DebugLevel::None);
        assert_eq!(cfg.max_response_body_bytes, Some(16 * 1024 * 1024));
        assert_eq!(cfg.max_stream_request_body_bytes, Some(16 * 1024 * 1024));
        assert_eq!(cfg.max_stream_response_body_bytes, Some(16 * 1024 * 1024));
        assert_eq!(cfg.max_rate_limit_cooldown, Duration::from_secs(60));
        #[cfg(feature = "dangerous-dev-tools")]
        assert!(cfg.dev_body_capture.is_none());

        assert_eq!(Arc::strong_count(&cfg.hooks), 1);
        assert_eq!(Arc::strong_count(&cfg.rate_limiter), 1);
        assert_eq!(Arc::strong_count(&cfg.retry_policy), 1);
        assert_eq!(Arc::strong_count(&cfg.debug.sink), 1);
    }
}
