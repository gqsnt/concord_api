use crate::cache::{CacheStore, NoopCacheStore};
use crate::debug::{DebugLevel, DebugSink, StderrDebugSink};
use crate::pagination::Caps;
use crate::rate_limit::{DefaultRateLimiter, RateLimiter};
use crate::retry::{NoRetryPolicy, RetryPolicy};
use crate::runtime_hooks::{NoopRuntimeHooks, RuntimeHooks};
use bytes::Bytes;
use http::{Method, StatusCode};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static DEV_BODY_CAPTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

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
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            level: DebugLevel::default(),
            sink: Arc::new(StderrDebugSink),
        }
    }
}

#[deprecated(
    note = "dev-only diagnostic capture; not for production; may persist sensitive response bytes to disk; disabled by default"
)]
#[derive(Clone, Debug)]
pub struct DevBodyCaptureConfig {
    pub(crate) dir: PathBuf,
    pub(crate) max_bytes: usize,
}

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

fn safe_capture_path(dir: &Path, filename: &str) -> PathBuf {
    debug_assert!(!filename.contains('/') && !filename.contains('\\'));
    dir.join(filename)
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
    #[allow(deprecated)]
    pub(crate) dev_body_capture: Option<DevBodyCaptureConfig>,
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

    #[deprecated(
        note = "dev-only diagnostic capture; not for production; may persist sensitive response bytes to disk; disabled by default"
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
    pub fn debug_level_value(&self) -> DebugLevel {
        self.debug.level
    }
}
