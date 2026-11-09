use concord::{ResponseMeta, reqwest::StatusCode};
use concord_retry::concord::ReceiveReport;
use serde::Deserialize;

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
    /// 429: frequency or concurrency limit, retried within bounds (reads and updates).
    RateLimited,
    /// Any 5xx; only 502, 503 and 504 are retried, and only for reads. An update answered with
    /// a 5xx is not retried and may have been applied: re-read the issue.
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
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub struct Problem {
    /// Provider explanation, possibly containing sensitive request content.
    pub detail: String,
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
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Invalid bearer token; nothing was sent.
    Credential,
    /// Invalid local policy settings; nothing was sent.
    #[from(skip)]
    Configuration(&'static str),
    /// Concord build, transport or successful-response decoding failure.
    Concord(Box<concord::Error>),
    /// Authoritative non-2xx failure.
    #[from(skip)]
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
    /// For an update this is never a dispatch left without an answer (that is `Uncertain`).
    RetryStopped {
        /// Dispatch and stop information.
        report: Box<ReceiveReport>,
        /// Last completed response head, if one arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// A status change outside the documented details; nothing was sent.
    #[from(skip)]
    InvalidUpdate(&'static str),
    /// A dispatched status update received no response head (a client timeout, reset or
    /// refused connection, or the SDK deadline expiring in flight). It may have been applied.
    /// It is never retried: re-read the issue to learn its status.
    Uncertain {
        /// The client's failure (`Delivery::Unknown`); `None` when the SDK deadline cancelled
        /// the dispatch. Its source may contain the URL.
        transport: Option<Box<concord::Error>>,
        /// All application attempts and waits; `no_response` is set.
        report: Box<ReceiveReport>,
    },
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Credential => f.write_str("invalid Sentry bearer token"),
        Error::Configuration(reason) => write!(f, "Sentry policy: {reason}"),
        Error::Concord(error) => write!(f, "Sentry call: {error}"),
        Error::Provider(error) => write!(
            f,
            "Sentry HTTP {} ({:?})",
            error.meta.status().as_u16(),
            error.class
        ),
        Error::Pagination { reason, .. } => write!(f, "Sentry pagination: {reason}"),
        Error::RateMetadata { reason, .. } => write!(f, "Sentry quota metadata: {reason}"),
        Error::Admission(error) => write!(f, "Sentry {error}"),
        Error::RetryStopped { report, .. } => {
            write!(f, "Sentry retry stopped: {:?}", report.retry.stop)
        }
        Error::InvalidUpdate(reason) => write!(f, "Sentry issue update: {reason}"),
        Error::Uncertain { .. } => {
            f.write_str("Sentry issue update outcome unknown: re-read the issue")
        }
    }
}
// Do not expose an untrusted provider formatter through a source chain.
