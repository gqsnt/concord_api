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
    /// Zero-based metadata index derived from the absolute physical attempt
    /// count; the first transport invocation is physical attempt 1.
    pub attempt: u32,
    pub retry_count: u32,
    pub page_index: u32,
    pub idempotent: bool,
    /// Maximum permitted server-directed delay. This is not a retry budget.
    pub max_delay: Duration,
    pub request_headers: &'a HeaderMap,
    pub response_headers: Option<&'a HeaderMap>,
    pub outcome: RetryOutcome<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    Stop,
    Retry,
}

/// Classifies retryable outcomes. Attempt ceilings and timing are owned by the
/// request execution state; a policy cannot add attempts or client waits.
pub trait RetryPolicy: Send + Sync + 'static {
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
    /// General attempts, including the initial send. The independently bounded
    /// authentication-recovery resend does not consume this capacity.
    pub max_attempts: u32,
    pub methods: Vec<Method>,
    pub statuses: Vec<StatusCode>,
    pub transport_errors: Vec<TransportErrorKind>,
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
        validate_max_attempts(self.max_attempts, ctx)
    }

    /// Computes only the outcome classification. The caller supplies the
    /// request-local general-attempt admission and any optional wait.
    pub fn try_decide(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        self.classifier().try_decide(ctx)
    }

    #[inline]
    pub fn decide(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        self.try_decide(ctx)
    }

    pub fn classifier(&self) -> RetryClassifierConfig {
        RetryClassifierConfig {
            methods: self.methods.clone(),
            statuses: self.statuses.clone(),
            transport_errors: self.transport_errors.clone(),
            idempotency: self.idempotency.clone(),
        }
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

/// Classification-only configuration for an inherited runtime retry policy.
/// General-attempt capacity and server-directed timing are resolved by the
/// runtime request configuration, never by this classifier.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetryClassifierConfig {
    pub methods: Vec<Method>,
    pub statuses: Vec<StatusCode>,
    pub transport_errors: Vec<TransportErrorKind>,
    pub idempotency: RetryIdempotency,
}

impl RetryClassifierConfig {
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
        Ok(if retryable {
            RetryDecision::Retry
        } else {
            RetryDecision::Stop
        })
    }

    fn method_allowed(&self, method: &Method) -> bool {
        self.methods.is_empty() || self.methods.iter().any(|m| m == method)
    }
}

#[derive(Clone, Debug)]
pub struct ConfiguredRetryPolicy {
    config: RetryClassifierConfig,
}

impl ConfiguredRetryPolicy {
    #[inline]
    pub fn new(config: RetryClassifierConfig) -> Self {
        Self { config }
    }
}

impl RetryPolicy for ConfiguredRetryPolicy {
    fn should_retry(&self, ctx: &RetryContext<'_>) -> RetryDecision {
        self.should_retry_checked(ctx)
            .unwrap_or(RetryDecision::Stop)
    }

    fn should_retry_checked(
        &self,
        ctx: &RetryContext<'_>,
    ) -> Result<RetryDecision, crate::error::ApiClientError> {
        self.config.try_decide(ctx)
    }
}

pub(crate) fn validate_max_attempts(
    max_attempts: u32,
    ctx: crate::error::ErrorContext,
) -> Result<(), crate::error::ApiClientError> {
    if !(1..=3).contains(&max_attempts) {
        return Err(crate::error::ApiClientError::invalid_param(
            ctx,
            "retry.max_attempts must be between 1 and 3",
        ));
    }
    Ok(())
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

pub(crate) fn validate_capped_retry_delay(
    ctx: &RetryContext<'_>,
    delay: Duration,
    max_delay: Duration,
    msg: &'static str,
) -> Result<(), crate::error::ApiClientError> {
    if delay > max_delay {
        return Err(retry_config_error(ctx, msg));
    }
    validate_retry_delay(ctx, delay, msg)
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
    ) -> RetryContext<'a> {
        RetryContext {
            endpoint: "RetryTest",
            method,
            url: "https://example.com",
            attempt: 1,
            retry_count: 0,
            page_index: 0,
            idempotent: true,
            max_delay: Duration::from_secs(60),
            request_headers: headers,
            response_headers,
            outcome: RetryOutcome::HttpStatus(StatusCode::TOO_MANY_REQUESTS),
        }
    }

    #[test]
    fn max_attempts_accepts_only_one_through_three() {
        let ctx = crate::error::ErrorContext {
            endpoint: "RetryTest",
            method: Method::GET,
        };
        for max_attempts in 1..=3 {
            assert!(
                RetryConfig {
                    max_attempts,
                    ..RetryConfig::default()
                }
                .validate(ctx.clone())
                .is_ok()
            );
        }
        for max_attempts in [0, 4, u32::MAX] {
            assert!(
                RetryConfig {
                    max_attempts,
                    ..RetryConfig::default()
                }
                .validate(ctx.clone())
                .is_err()
            );
        }
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
                    Some(&response_headers)
                ))
                .unwrap(),
            RetryDecision::Retry
        );
    }

    #[test]
    fn configured_transport_kinds_match_only_the_primary_classification() {
        let method = Method::GET;
        let request_headers = HeaderMap::new();
        for kind in [
            TransportErrorKind::Timeout,
            TransportErrorKind::Connect,
            TransportErrorKind::Tls,
            TransportErrorKind::Dns,
            TransportErrorKind::Io,
            TransportErrorKind::Request,
            TransportErrorKind::Other,
        ] {
            let error = TransportError::with_kind(kind, std::io::Error::other("classified"));
            let context = RetryContext {
                endpoint: "RetryTransportKind",
                method: &method,
                url: "https://example.com",
                attempt: 0,
                retry_count: 0,
                page_index: 0,
                idempotent: true,
                max_delay: Duration::from_secs(1),
                request_headers: &request_headers,
                response_headers: None,
                outcome: RetryOutcome::Transport(&error),
            };
            let matching = RetryClassifierConfig {
                transport_errors: vec![kind],
                ..RetryClassifierConfig::default()
            };
            assert_eq!(
                matching.try_decide(&context).expect("matching decision"),
                RetryDecision::Retry,
                "{kind:?} must match itself"
            );
            let other = if kind == TransportErrorKind::Other {
                TransportErrorKind::Request
            } else {
                TransportErrorKind::Other
            };
            let mismatching = RetryClassifierConfig {
                transport_errors: vec![other],
                ..RetryClassifierConfig::default()
            };
            assert_eq!(
                mismatching
                    .try_decide(&context)
                    .expect("mismatching decision"),
                RetryDecision::Stop,
                "{kind:?} must not match {other:?}"
            );
        }
    }
}
