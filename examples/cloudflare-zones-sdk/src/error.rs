//! The SDK's error and response metadata.

use std::fmt;
use std::num::NonZeroU32;

use concord::reqwest::StatusCode;
use concord::reqwest::header::HeaderMap;
use concord::{ErrorKind, Operation};

use crate::ratelimit::{RateLimitHeaderError, RateLimits, RetryAfter};
use crate::wire::{ErrorEnvelope, ResponseInfo};

/// The head of a response: status, headers and the operation, kept after the body is consumed.
///
/// `Debug` prints the status, the HTTP version, the header **names** and the operation, never a
/// header value (Cloudflare sets cookies). Values are read deliberately through
/// [`headers`](Self::headers).
#[derive(Clone)]
pub struct ResponseMeta(concord::ResponseMeta);

impl ResponseMeta {
    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.0.status()
    }

    /// The header fields, unredacted.
    pub fn headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    /// The declared operation (`Cloudflare.zone.get`, ...).
    pub fn operation(&self) -> Option<Operation> {
        self.0.operation()
    }

    /// Every `Ratelimit` and `Ratelimit-Policy` item, by name. Global and per-service limits
    /// are separate items.
    ///
    /// # Errors
    ///
    /// [`RateLimitHeaderError`] when a header does not follow the documented syntax.
    pub fn rate_limits(&self) -> Result<RateLimits, RateLimitHeaderError> {
        RateLimits::from_headers(self.headers())
    }

    /// `Retry-After`, sent when the request exceeded a limit.
    pub fn retry_after(&self) -> RetryAfter {
        RetryAfter::from_headers(self.headers())
    }

    /// Concord's copy of the head.
    pub fn as_concord(&self) -> &concord::ResponseMeta {
        &self.0
    }
}

impl From<concord::ResponseMeta> for ResponseMeta {
    fn from(meta: concord::ResponseMeta) -> Self {
        Self(meta)
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// A response Cloudflare answered with a refusal: `success: false` in an HTTP 2xx envelope, or
/// a non-2xx status.
#[derive(Debug)]
pub struct ProviderError {
    meta: ResponseMeta,
    document: Result<ErrorEnvelope, concord::Error>,
    attempts: usize,
}

impl ProviderError {
    pub(crate) fn new(
        meta: ResponseMeta,
        document: Result<ErrorEnvelope, concord::Error>,
        attempts: usize,
    ) -> Self {
        Self {
            meta,
            document,
            attempts,
        }
    }

    /// The HTTP status: 2xx when the envelope said `success: false`.
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The error envelope, or why the body could not be read as one (an HTML page, an empty or
    /// oversized body: `ErrorKind::Metadata`, `Decode`, `Limit`). Always `Ok` for a 2xx.
    pub fn document(&self) -> Result<&ErrorEnvelope, &concord::Error> {
        self.document.as_ref()
    }

    /// `errors`, in order; empty when the body was unreadable or listed none.
    pub fn errors(&self) -> &[ResponseInfo] {
        self.document.as_ref().map_or(&[], |d| &d.errors)
    }

    /// `messages`, in order.
    pub fn messages(&self) -> &[ResponseInfo] {
        self.document.as_ref().map_or(&[], |d| &d.messages)
    }

    /// Whether the error carries `code`.
    pub fn has_code(&self, code: u64) -> bool {
        self.errors().iter().any(|e| e.code == code)
    }

    /// `Retry-After` of this response.
    pub fn retry_after(&self) -> RetryAfter {
        self.meta.retry_after()
    }

    /// Requests sent for this call: more than one only when the send policy honoured a
    /// `Retry-After` ([`SendPolicy::retry_rate_limited`](crate::SendPolicy::retry_rate_limited)).
    pub fn attempts(&self) -> usize {
        self.attempts
    }
}

impl fmt::Display for ProviderError {
    /// Status, operation and error codes; never a message or header value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = self.meta.operation().map_or("a request", |o| o.name());
        write!(
            f,
            "Cloudflare refused {op}: HTTP {}",
            self.status().as_u16()
        )?;
        if self.status().is_success() {
            f.write_str(", success=false")?;
        }
        match &self.document {
            Ok(document) => write!(f, ", error codes {:?}", document.codes()),
            Err(_) => f.write_str(", error document unreadable"),
        }
    }
}

/// An HTTP 2xx `success: true` envelope the SDK cannot use.
#[derive(Debug)]
pub struct EnvelopeError {
    meta: ResponseMeta,
    problem: EnvelopeProblem,
}

impl EnvelopeError {
    pub(crate) fn new(meta: ResponseMeta, problem: EnvelopeProblem) -> Self {
        Self { meta, problem }
    }

    /// What is wrong.
    pub fn problem(&self) -> EnvelopeProblem {
        self.problem
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

/// Why a `success: true` envelope is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeProblem {
    /// `result` is absent or `null`: never read as an empty listing or a missing object.
    MissingResult,
    /// A page walker needs `result_info.page` and `result_info.total_pages`.
    MissingPageInfo,
    /// The page Cloudflare answered is not the page requested.
    PageMismatch {
        /// The page requested.
        requested: NonZeroU32,
        /// `result_info.page`.
        returned: u32,
    },
}

impl fmt::Display for EnvelopeProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingResult => f.write_str("success without a result"),
            Self::MissingPageInfo => f.write_str("listing without result_info.page/total_pages"),
            Self::PageMismatch {
                requested,
                returned,
            } => write!(f, "page {requested} requested, page {returned} returned"),
        }
    }
}

/// Every failure of this SDK.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Cloudflare refused the request: `success: false` (whatever the 2xx status), or a non-2xx
    /// status other than 429. The status is authoritative even when the body is unreadable.
    Provider(Box<ProviderError>),
    /// HTTP 429: a global or a service limit was exceeded. Read
    /// [`ProviderError::retry_after`] and [`ResponseMeta::rate_limits`].
    RateLimited(Box<ProviderError>),
    /// An HTTP 2xx `success: true` envelope without what the call needs.
    Envelope(Box<EnvelopeError>),
    /// The API token is not a valid bearer credential; nothing was sent.
    Credential(concord_auth::CredentialError),
    /// Concord's failure: `Build` (nothing sent), `Transport`, `Metadata` (a 2xx that is not
    /// JSON), `Decode` (a body that is not an envelope), `Limit` (a body over
    /// [`SUCCESS_BODY_LIMIT`](crate::SUCCESS_BODY_LIMIT)). Once a response head arrived,
    /// `error.head()` has it.
    Concord(concord::Error),
    /// The retry owner completed no attempt. Not expected with this SDK's policy (no deadline,
    /// requests without a body); kept so that its outcome is mapped totally.
    NoAttempt,
}

impl Error {
    /// The provider refusal, for [`Error::Provider`] and [`Error::RateLimited`].
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Provider(e) | Self::RateLimited(e) => Some(e),
            _ => None,
        }
    }

    /// The HTTP status, once a response head arrived.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Provider(e) | Self::RateLimited(e) => Some(e.status()),
            Self::Envelope(e) => Some(e.meta.status()),
            Self::Concord(e) => e.head().map(|head| head.status()),
            Self::Credential(_) | Self::NoAttempt => None,
        }
    }

    /// Concord's error kind, for [`Error::Concord`].
    pub fn concord_kind(&self) -> Option<ErrorKind> {
        match self {
            Self::Concord(e) => Some(e.kind()),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(e) => e.fmt(f),
            Self::RateLimited(e) => {
                e.fmt(f)?;
                match e.retry_after() {
                    RetryAfter::After(delay) => write!(f, " (rate limited, retry after {delay:?})"),
                    RetryAfter::Absent => f.write_str(" (rate limited, no Retry-After)"),
                    RetryAfter::Invalid => f.write_str(" (rate limited, invalid Retry-After)"),
                }
            }
            Self::Envelope(e) => {
                let op = e.meta.operation().map_or("a request", |o| o.name());
                write!(f, "unusable Cloudflare envelope for {op}: {}", e.problem)
            }
            Self::Credential(e) => write!(f, "invalid Cloudflare API token: {e}"),
            Self::Concord(e) => e.fmt(f),
            Self::NoAttempt => f.write_str("no attempt completed"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Concord(e) => Some(e),
            Self::Credential(e) => Some(e),
            _ => None,
        }
    }
}
