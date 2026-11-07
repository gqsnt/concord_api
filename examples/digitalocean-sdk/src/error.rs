use crate::ResponseMeta;
use serde::Deserialize;
use std::fmt;

/// An error document. Explicit field access discloses untrusted provider text.
#[derive(Deserialize)]
pub struct Problem {
    /// Provider error identifier; HTTP status remains authoritative.
    pub id: String,
    /// Provider-supplied explanation, withheld from formatting.
    pub message: String,
    /// Optional diagnostic request identifier.
    pub request_id: Option<String>,
}
impl fmt::Debug for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Problem(<redacted>)")
    }
}

/// Classification by HTTP status, independent of document contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpClass {
    /// HTTP 400.
    BadRequest,
    /// HTTP 401; the SDK does not acquire or refresh tokens.
    Unauthorized,
    /// HTTP 403.
    Forbidden,
    /// HTTP 404.
    NotFound,
    /// HTTP 429; delay metadata is exposed, with no automatic replay.
    RateLimited,
    /// Any HTTP 5xx.
    Server,
    /// Any other non-success (including redirects and default errors).
    Other,
}

/// A non-success status, retaining either the typed document or why it was unreadable.
#[derive(Debug)]
pub struct ProviderError {
    /// The original HTTP metadata.
    pub meta: ResponseMeta,
    /// Strict JSON document outcome; malformed, missing, wrong-media, oversized and
    /// interrupted bodies leave the HTTP failure intact.
    pub document: Result<Problem, concord::Error>,
}
impl ProviderError {
    /// Classify using status, even if the document contradicts it or cannot be decoded.
    pub fn class(&self) -> HttpClass {
        match self.meta.status().as_u16() {
            400 => HttpClass::BadRequest,
            401 => HttpClass::Unauthorized,
            403 => HttpClass::Forbidden,
            404 => HttpClass::NotFound,
            429 => HttpClass::RateLimited,
            500..=599 => HttpClass::Server,
            _ => HttpClass::Other,
        }
    }
}

/// SDK failures; formatting never includes credentials, response text or link targets.
#[derive(Debug)]
pub enum Error {
    /// Invalid bearer token syntax, before I/O.
    Credential,
    /// Fixture destination must be a bare HTTP loopback origin.
    FixtureOrigin,
    /// Transport construction failed; no request was sent.
    TransportSetup,
    /// Page numbers are one-based.
    PageNumber,
    /// Page size must be in 1..=200.
    PageSize,
    /// Present next URL is invalid or leaves the trusted origin/collection.
    UnsafeNext,
    /// Concord build, transport, metadata, decode or size-limit failure.
    Concord(concord::Error),
    /// HTTP failure; status is authoritative even when its document fails.
    Provider(Box<ProviderError>),
}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(error)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credential => f.write_str("invalid bearer credential"),
            Self::FixtureOrigin => f.write_str("invalid loopback fixture origin"),
            Self::TransportSetup => f.write_str("could not configure transport"),
            Self::PageNumber => f.write_str("page number must be positive"),
            Self::PageSize => f.write_str("page size must be between 1 and 200"),
            Self::UnsafeNext => f.write_str("unsafe pagination target"),
            Self::Concord(error) => error.fmt(f),
            Self::Provider(error) => write!(
                f,
                "DigitalOcean HTTP {} ({:?})",
                error.meta.status(),
                error.class()
            ),
        }
    }
}
// Deliberately no automatic source chain through untrusted provider documents.
impl std::error::Error for Error {}
