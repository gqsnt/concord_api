use crate::transport::{TransportError, TransportErrorKind};
use http::header::HeaderName;
use http::{HeaderMap, Method, StatusCode};
use std::time::{Duration, Instant};

#[allow(dead_code)]
pub type RetryPlan = RetryConfig;

#[derive(Debug)]
pub enum RetryOutcome<'a> {
    Transport(&'a TransportError),
    HttpStatus(StatusCode),
    Decode,
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

    #[inline]
    fn should_retry_checked(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        Ok(self.should_retry(ctx))
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
    pub max_attempts: u32,
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
            max_attempts: 1,
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
    pub fn validate(
        &self,
        ctx: crate::error::ErrorContext,
    ) -> Result<(), crate::error::ApiClientError> {
        if self.max_attempts == 0 {
            return Err(crate::error::ApiClientError::invalid_param(
                ctx,
                "retry.max_attempts must be at least 1",
            ));
        }
        Ok(())
    }

    #[inline]
    pub fn max_retries(&self) -> u32 {
        if self.max_attempts == 0 {
            0
        } else {
            self.max_attempts - 1
        }
    }

    /// Computes the retry decision and returns a typed error for invalid or
    /// overflowing retry delays.
    pub fn try_decide(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        if !self.method_allowed(ctx.method) || !self.idempotency.allows(ctx) {
            return Ok(RetryDecision::Stop);
        }

        let retryable = match &ctx.outcome {
            RetryOutcome::HttpStatus(status) => self.statuses.iter().any(|s| s == status),
            RetryOutcome::Transport(err) => {
                self.transport_errors.iter().any(|kind| *kind == err.kind())
            }
            RetryOutcome::Decode | RetryOutcome::Other => false,
        };
        if !retryable {
            return Ok(RetryDecision::Stop);
        }

        if self.respect_retry_after
            && let Some(headers) = ctx.response_headers
            && let Some(delay) = crate::rate_limit::parse_retry_after(headers)
        {
            validate_retry_delay(ctx, delay, "retry Retry-After duration overflowed")?;
            return Ok(RetryDecision::RetryAfter(delay));
        }

        match self.backoff.delay(ctx)? {
            Some(delay) if !delay.is_zero() => Ok(RetryDecision::RetryAfter(delay)),
            _ => Ok(RetryDecision::Retry),
        }
    }

    /// Computes the retry decision using the checked v1 retry API.
    #[inline]
    pub fn decide(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        self.try_decide(ctx)
    }

    fn method_allowed(&self, method: &Method) -> bool {
        self.methods.is_empty() || self.methods.iter().any(|m| m == method)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RetryIdempotency {
    #[default]
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

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RetryBackoff {
    #[default]
    None,
    Fixed(Duration),
    Exponential {
        base: Duration,
        factor: f64,
        max: Duration,
    },
}

impl RetryBackoff {
    fn delay(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<Option<Duration>, crate::error::ApiClientError> {
        match self {
            Self::None => Ok(Some(Duration::ZERO)),
            Self::Fixed(delay) => {
                validate_retry_delay(ctx, *delay, "retry fixed backoff duration overflowed")?;
                Ok(Some(*delay))
            }
            Self::Exponential { base, factor, max } => {
                if !factor.is_finite() || *factor < 0.0 {
                    return Err(retry_config_error(
                        ctx,
                        "retry exponential backoff factor must be finite and non-negative",
                    ));
                }
                let multiplier = factor.powi(ctx.retry_count.min(i32::MAX as u32) as i32);
                let seconds = base.as_secs_f64() * multiplier;
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err(retry_config_error(
                        ctx,
                        "retry exponential backoff duration overflowed",
                    ));
                }
                let delay = Duration::try_from_secs_f64(seconds).map_err(|_| {
                    retry_config_error(ctx, "retry exponential backoff duration overflowed")
                })?;
                let delay = delay.min(*max);
                validate_retry_delay(ctx, delay, "retry exponential backoff duration overflowed")?;
                Ok(Some(delay))
            }
        }
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
        self.config.try_decide(ctx).unwrap_or(RetryDecision::Stop)
    }

    fn should_retry_checked(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        self.config.try_decide(ctx)
    }
}

pub(crate) fn validate_retry_delay(
    ctx: &RetryContext<'_>,
    delay: Duration,
    msg: &'static str,
) -> Result<(), crate::error::ApiClientError> {
    Instant::now()
        .checked_add(delay)
        .map(|_| ())
        .ok_or_else(|| retry_config_error(ctx, msg))
}

fn retry_config_error(ctx: &RetryContext<'_>, msg: &'static str) -> crate::error::ApiClientError {
    crate::error::ApiClientError::invalid_param(
        crate::error::ErrorContext {
            endpoint: ctx.endpoint,
            method: ctx.method.clone(),
        },
        msg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        method: &'a Method,
        headers: &'a HeaderMap,
        response_headers: Option<&'a HeaderMap>,
        retry_count: u32,
    ) -> RetryContext<'a> {
        RetryContext {
            endpoint: "RetryTest",
            method,
            url: "https://example.com",
            attempt: retry_count,
            retry_count,
            page_index: 0,
            idempotent: true,
            request_headers: headers,
            response_headers,
            outcome: RetryOutcome::HttpStatus(StatusCode::TOO_MANY_REQUESTS),
        }
    }

    #[test]
    fn max_attempts_counts_the_first_send() {
        let config = RetryConfig {
            max_attempts: 1,
            statuses: vec![StatusCode::TOO_MANY_REQUESTS],
            methods: vec![Method::GET],
            ..RetryConfig::default()
        };
        assert_eq!(config.max_retries(), 0);

        let config = RetryConfig {
            max_attempts: 2,
            statuses: vec![StatusCode::TOO_MANY_REQUESTS],
            methods: vec![Method::GET],
            ..RetryConfig::default()
        };
        assert_eq!(config.max_retries(), 1);
    }

    #[test]
    fn retry_after_is_honored_when_enabled() {
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("3"),
        );
        let request_headers = HeaderMap::new();
        let config = RetryConfig {
            max_attempts: 2,
            statuses: vec![StatusCode::TOO_MANY_REQUESTS],
            methods: vec![Method::GET],
            respect_retry_after: true,
            ..RetryConfig::default()
        };

        assert_eq!(
            config
                .decide(&ctx(
                    &Method::GET,
                    &request_headers,
                    Some(&response_headers),
                    0
                ))
                .expect("retry decision should be valid"),
            RetryDecision::RetryAfter(Duration::from_secs(3))
        );
    }

    #[test]
    fn retry_after_http_date_is_honored_when_enabled() {
        let mut response_headers = HeaderMap::new();
        let when = std::time::SystemTime::now() + Duration::from_secs(3);
        response_headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_str(&httpdate::fmt_http_date(when))
                .expect("valid retry-after date"),
        );
        let request_headers = HeaderMap::new();
        let config = RetryConfig {
            max_attempts: 2,
            statuses: vec![StatusCode::TOO_MANY_REQUESTS],
            methods: vec![Method::GET],
            respect_retry_after: true,
            ..RetryConfig::default()
        };

        let decision = config
            .try_decide(&ctx(
                &Method::GET,
                &request_headers,
                Some(&response_headers),
                0,
            ))
            .expect("http-date retry-after should be valid");
        let RetryDecision::RetryAfter(delay) = decision else {
            panic!("expected retry-after decision");
        };
        assert!(delay <= Duration::from_secs(3));
    }

    #[test]
    fn huge_retry_after_returns_typed_error() {
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("18446744073709551615"),
        );
        let request_headers = HeaderMap::new();
        let config = RetryConfig {
            max_attempts: 2,
            statuses: vec![StatusCode::TOO_MANY_REQUESTS],
            methods: vec![Method::GET],
            respect_retry_after: true,
            ..RetryConfig::default()
        };

        let err = config
            .try_decide(&ctx(
                &Method::GET,
                &request_headers,
                Some(&response_headers),
                0,
            ))
            .expect_err("huge retry-after should fail");
        assert_eq!(err.category(), crate::error::ErrorCategory::Config);
        assert!(err.to_string().contains("Retry-After"));
    }

    #[test]
    fn exponential_backoff_overflow_returns_typed_error() {
        let request_headers = HeaderMap::new();
        let config = RetryConfig {
            max_attempts: 2,
            statuses: vec![StatusCode::TOO_MANY_REQUESTS],
            methods: vec![Method::GET],
            backoff: RetryBackoff::Exponential {
                base: Duration::MAX,
                factor: f64::INFINITY,
                max: Duration::MAX,
            },
            ..RetryConfig::default()
        };

        let err = config
            .try_decide(&ctx(&Method::GET, &request_headers, None, 1))
            .expect_err("invalid exponential backoff should fail");
        assert_eq!(err.category(), crate::error::ErrorCategory::Config);
        assert!(err.to_string().contains("backoff"));
    }

    #[test]
    fn zero_max_attempts_is_invalid() {
        let config = RetryConfig {
            max_attempts: 0,
            ..RetryConfig::default()
        };
        let ctx = crate::error::ErrorContext {
            endpoint: "RetryTest",
            method: Method::GET,
        };

        let err = config.validate(ctx).expect_err("zero max_attempts fails");
        assert_eq!(err.category(), crate::error::ErrorCategory::Config);
        assert!(err.to_string().contains("retry.max_attempts"));
    }
}
