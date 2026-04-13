use crate::transport::{TransportError, TransportErrorKind};
use http::header::{HeaderName, RETRY_AFTER};
use http::{HeaderMap, Method, StatusCode};
use std::time::Duration;

#[derive(Debug)]
pub enum RetryOutcome<'a> {
    Transport(&'a TransportError),
    HttpStatus(StatusCode),
    Decode,
    Transform,
    Other,
}

#[derive(Debug)]
pub struct RetryContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub attempt: u32,
    pub retry_count: u32,
    pub page_index: u32,
    pub idempotent: bool,
    pub request_headers: &'a HeaderMap,
    pub response_headers: Option<&'a HeaderMap>,
    pub outcome: RetryOutcome<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    Stop,
    Retry,
    RetryAfter(Duration),
}

pub trait RetryPolicy: Send + Sync + 'static {
    #[inline]
    fn max_retries(&self) -> u32 {
        0
    }

    #[inline]
    fn should_retry(&self, _ctx: &RetryContext<'_>) -> RetryDecision {
        RetryDecision::Stop
    }
}

#[derive(Default)]
pub struct NoRetryPolicy;

impl RetryPolicy for NoRetryPolicy {}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RetrySetting {
    #[default]
    Inherit,
    Config(RetryConfig),
    Off,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetryConfig {
    pub attempts: u32,
    pub methods: Vec<Method>,
    pub statuses: Vec<StatusCode>,
    pub transport_errors: Vec<TransportErrorKind>,
    pub backoff: RetryBackoff,
    pub respect_retry_after: bool,
    pub idempotency: RetryIdempotency,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            attempts: 1,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: Vec::new(),
            backoff: RetryBackoff::None,
            respect_retry_after: false,
            idempotency: RetryIdempotency::SafeMethodsOnly,
        }
    }
}

impl RetryConfig {
    #[inline]
    pub fn max_retries(&self) -> u32 {
        self.attempts.saturating_sub(1)
    }

    pub fn decide(&self, ctx: &RetryContext<'_>) -> RetryDecision {
        if !self.method_allowed(ctx.method) || !self.idempotency.allows(ctx) {
            return RetryDecision::Stop;
        }

        let retryable = match &ctx.outcome {
            RetryOutcome::HttpStatus(status) => self.statuses.iter().any(|s| s == status),
            RetryOutcome::Transport(err) => {
                self.transport_errors.iter().any(|kind| *kind == err.kind())
            }
            RetryOutcome::Decode | RetryOutcome::Transform | RetryOutcome::Other => false,
        };
        if !retryable {
            return RetryDecision::Stop;
        }

        if self.respect_retry_after
            && let Some(headers) = ctx.response_headers
            && let Some(delay) = retry_after_delay(headers)
        {
            return RetryDecision::RetryAfter(delay);
        }

        match self.backoff.delay(ctx.retry_count) {
            Some(delay) if !delay.is_zero() => RetryDecision::RetryAfter(delay),
            _ => RetryDecision::Retry,
        }
    }

    fn method_allowed(&self, method: &Method) -> bool {
        self.methods.is_empty() || self.methods.iter().any(|m| m == method)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetryIdempotency {
    SafeMethodsOnly,
    Header(HeaderName),
}

impl RetryIdempotency {
    fn allows(&self, ctx: &RetryContext<'_>) -> bool {
        if ctx.idempotent {
            return true;
        }
        match self {
            Self::SafeMethodsOnly => false,
            Self::Header(name) => ctx.request_headers.contains_key(name),
        }
    }
}

impl Default for RetryIdempotency {
    fn default() -> Self {
        Self::SafeMethodsOnly
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetryBackoff {
    None,
    Fixed(Duration),
    Exponential {
        base: Duration,
        factor: f64,
        max: Duration,
    },
}

impl RetryBackoff {
    fn delay(&self, retry_count: u32) -> Option<Duration> {
        match self {
            Self::None => Some(Duration::ZERO),
            Self::Fixed(delay) => Some(*delay),
            Self::Exponential { base, factor, max } => {
                let multiplier = factor.powi(retry_count.min(i32::MAX as u32) as i32);
                let delay = Duration::from_secs_f64(base.as_secs_f64() * multiplier);
                Some(delay.min(*max))
            }
        }
    }
}

impl Default for RetryBackoff {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub struct ConfiguredRetryPolicy {
    config: RetryConfig,
}

impl ConfiguredRetryPolicy {
    #[inline]
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    #[inline]
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }
}

impl RetryPolicy for ConfiguredRetryPolicy {
    fn max_retries(&self) -> u32 {
        self.config.max_retries()
    }

    fn should_retry(&self, ctx: &RetryContext<'_>) -> RetryDecision {
        self.config.decide(ctx)
    }
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds = raw.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}
