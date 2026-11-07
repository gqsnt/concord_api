use std::fmt;

use concord::ResponseMeta;
use serde::Deserialize;

/// MediaWiki's application error. Fields are explicit, unredacted provider data.
#[derive(Deserialize)]
pub struct ApiProblem {
    /// Machine-readable provider code (open-ended).
    pub code: String,
    /// Human-readable provider detail; may echo request data.
    pub info: String,
}

impl fmt::Debug for ApiProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiProblem(<redacted>)")
    }
}

/// The expected error envelope on a non-success HTTP response.
#[derive(Debug, Deserialize)]
pub struct ErrorDocument {
    /// Explicitly inspectable provider error, with redacted Debug.
    pub error: ApiProblem,
}

/// A non-2xx remains an HTTP failure even when its document cannot be read or decoded.
#[derive(Debug)]
pub struct HttpFailure {
    /// The authoritative HTTP status and response head.
    pub meta: ResponseMeta,
    /// Typed provider error or the reason its body was refused.
    pub document: Result<ErrorDocument, concord::Error>,
}

/// Errors from generated `send()` and checked content projection. No automatic retries.
#[derive(Debug)]
pub enum Error {
    /// Build, transport, metadata, decode or body-limit failure, preserving its operation/head.
    Concord(Box<concord::Error>),
    /// Non-success HTTP response, including 429 and 5xx, with a bounded document attempt.
    Http(Box<HttpFailure>),
    /// MediaWiki's error envelope in a successful HTTP response.
    Api {
        /// Response head, normally HTTP 200.
        meta: Box<ResponseMeta>,
        /// Explicit unredacted access to the provider code and info.
        problem: ApiProblem,
    },
    /// Any warnings envelope is refused to avoid accepting a possibly partial query.
    Warnings(Box<ResponseMeta>),
    /// A missing query object, or an inconsistent single-page or batch content response.
    InvalidResponse(Box<ResponseMeta>),
    /// Missing/invalid page, absent revision, or unavailable/suppressed main-slot content.
    ContentUnavailable,
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(Box::new(error))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(error) => error.fmt(f),
            Self::Http(failure) => write!(f, "MediaWiki HTTP failure ({})", failure.meta.status()),
            Self::Api { .. } => f.write_str("MediaWiki returned an API error"),
            Self::Warnings(_) => f.write_str("MediaWiki returned query warnings"),
            Self::InvalidResponse(_) => {
                f.write_str("MediaWiki returned an inconsistent query response")
            }
            Self::ContentUnavailable => f.write_str("the chosen page's content is unavailable"),
        }
    }
}

// Deliberately no source chain: explicit variants/accessors retain unredacted provider data.
impl std::error::Error for Error {}
