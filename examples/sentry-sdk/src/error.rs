use concord::{ResponseMeta, reqwest::StatusCode};
use concord_retry::concord::ReceiveReport;
use serde::Deserialize;
use std::fmt;

/// HTTP failure classification; status remains authoritative if its document cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpClass {
    /// 400: invalid query or other request input.
    BadRequest,
    /// 401: missing/invalid/expired token; no refresh or retry.
    Unauthorized,
    /// 403: insufficient scope/access; no retry.
    Forbidden,
    /// 404: missing or inaccessible resource; no retry.
    NotFound,
    /// 429: frequency or concurrency limit, retried within bounds.
    RateLimited,
    /// Any 5xx; only 502, 503 and 504 are retried.
    Server,
    /// Any other non-2xx, including redirects.
    Other,
}
impl HttpClass {
    /// Classify without inspecting untrusted body text.
    pub fn of(status: StatusCode) -> Self {
        match status.as_u16() {
            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            429 => Self::RateLimited,
            500..=599 => Self::Server,
            _ => Self::Other,
        }
    }
}

/// Small provider error document. Its text is available explicitly, withheld from formatting.
#[derive(Deserialize)]
pub struct Problem {
    /// Provider explanation, possibly containing sensitive request content.
    pub detail: String,
}
impl fmt::Debug for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Problem { <withheld> }")
    }
}

/// HTTP failure with a typed or unreadable bounded document.
#[derive(Debug)]
pub struct ProviderError {
    /// HTTP error category.
    pub class: HttpClass,
    /// Actual status and headers, even when document parsing fails.
    pub meta: ResponseMeta,
    /// JSON document, or build-independent body/media/transport/size diagnostic.
    pub document: Result<Problem, concord::Error>,
    /// Invalid quota or retry headers prevented retries; contains static text only.
    pub scheduling_error: Option<&'static str>,
    /// All application attempts and waits.
    pub retry: ReceiveReport,
}

/// SDK errors. Formatting never includes token, URL, cursor, headers or body content.
#[derive(Debug)]
pub enum Error {
    /// Invalid bearer token; nothing was sent.
    Credential,
    /// Invalid local policy settings; nothing was sent.
    Configuration(&'static str),
    /// Concord build, transport or successful-response decoding failure.
    Concord(Box<concord::Error>),
    /// Authoritative non-2xx failure.
    Provider(Box<ProviderError>),
    /// Invalid Sentry Link semantics; the head is retained.
    Pagination {
        /// Static diagnostic.
        reason: &'static str,
        /// Head of the page that could not be continued safely.
        meta: Box<ResponseMeta>,
    },
    /// Invalid Sentry quota metadata on a 2xx response.
    RateMetadata {
        /// Static diagnostic.
        reason: &'static str,
        /// Actual response head.
        meta: Box<ResponseMeta>,
    },
    /// Process-local admission failure.
    Admission(concord_ratelimit::Error),
    /// Total deadline or replay refusal; no stale response body is consumed without a lease.
    RetryStopped {
        /// Dispatch and stop information.
        report: Box<ReceiveReport>,
        /// Last completed response head, if one arrived.
        meta: Option<Box<ResponseMeta>>,
    },
}
impl From<concord::Error> for Error {
    fn from(value: concord::Error) -> Self {
        Self::Concord(Box::new(value))
    }
}
impl From<concord_ratelimit::Error> for Error {
    fn from(value: concord_ratelimit::Error) -> Self {
        Self::Admission(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credential => f.write_str("invalid Sentry bearer token"),
            Self::Configuration(reason) => write!(f, "Sentry policy: {reason}"),
            Self::Concord(error) => write!(f, "Sentry call: {error}"),
            Self::Provider(error) => write!(
                f,
                "Sentry HTTP {} ({:?})",
                error.meta.status().as_u16(),
                error.class
            ),
            Self::Pagination { reason, .. } => write!(f, "Sentry pagination: {reason}"),
            Self::RateMetadata { reason, .. } => write!(f, "Sentry quota metadata: {reason}"),
            Self::Admission(error) => write!(f, "Sentry {error}"),
            Self::RetryStopped { report, .. } => {
                write!(f, "Sentry retry stopped: {:?}", report.retry.stop)
            }
        }
    }
}
// Do not expose an untrusted provider formatter through a source chain.
impl std::error::Error for Error {}
