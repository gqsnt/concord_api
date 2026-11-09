//! Typed failures with value-free diagnostics and explicit raw access.

use concord::{ResponseMeta, reqwest::StatusCode};
use serde::Deserialize;
use std::{
    fmt,
    time::{Duration, SystemTime},
};

/// Local refusals. No variant stores a credential, selector, body or URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_local_error)]
pub enum LocalError {
    /// API key is empty or not an ASCII token.
    InvalidKey,
    /// Secret is empty or not standard Base64.
    InvalidSecret,
    /// A private request has no credentials.
    MissingCredentials,
    /// The shared counter has reached u64::MAX.
    NonceExhausted,
    /// A prepared request cannot be signed by this bounded policy.
    SigningInput,
    /// Pair selector has zero matches, several matches, or an offline match.
    PairUnavailable,
    /// Ticker response does not contain exactly the selected canonical market.
    TickerMismatch,
}
fn display_local_error(value: &LocalError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Kraken local refusal: {value:?}")
}

/// Stable SDK classifications; raw unknown provider errors are retained separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiClass {
    /// Invalid API key or signature.
    Authentication,
    /// Provider rejected an old or repeated nonce; never automatically replayed.
    InvalidNonce,
    /// Missing permission.
    Permission,
    /// Provider rate limit or throttling refusal.
    RateLimited,
    /// Temporary or permanent service failure.
    Service,
    /// Invalid arguments.
    InvalidArguments,
    /// Unrecognized provider string, including warnings (conservative failure).
    Other,
}

/// Classifies only known prefixes; it never echoes untrusted suffixes.
pub fn classify(message: &str) -> ApiClass {
    if message == "EAPI:Invalid nonce" {
        ApiClass::InvalidNonce
    } else if matches!(message, "EAPI:Invalid key" | "EAPI:Invalid signature") {
        ApiClass::Authentication
    } else if message == "EGeneral:Permission denied" {
        ApiClass::Permission
    } else if message == "EAPI:Rate limit exceeded" || message.starts_with("EService:Throttled:") {
        ApiClass::RateLimited
    } else if message.starts_with("EService:") {
        ApiClass::Service
    } else if message.starts_with("EGeneral:Invalid arguments") {
        ApiClass::InvalidArguments
    } else {
        ApiClass::Other
    }
}

/// Bounded HTTP error document; Debug withholds all provider strings.
#[derive(Deserialize)]
pub struct ErrorDocument {
    error: Vec<String>,
}
impl ErrorDocument {
    /// Explicit unredacted provider strings; do not send these to ordinary logs.
    pub fn errors_unredacted(&self) -> &[String] {
        &self.error
    }
}
impl fmt::Debug for ErrorDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorDocument")
            .field("error_count", &self.error.len())
            .finish()
    }
}

/// A non-2xx answer remains an HTTP failure even if its error body is unreadable.
#[derive(Debug)]
pub struct HttpFailure {
    /// Status, operation and headers; metadata Debug prints header names only.
    pub meta: ResponseMeta,
    /// JSON document or its media/limit/transport/decode failure.
    pub document: Result<ErrorDocument, concord::Error>,
}
impl HttpFailure {
    /// Projects all Retry-After field lines using Concord's shared strict parser.
    /// This is advisory information; it does not initiate a retry or a sleep.
    pub fn retry_after(&self, now: SystemTime) -> Result<Option<Duration>, crate::RetryAfterError> {
        concord_retry::http::retry_after(
            self.meta
                .headers()
                .get_all("retry-after")
                .iter()
                .map(|v| v.as_bytes()),
            now,
        )
    }
}

/// A nonempty Kraken error list inside a successful HTTP response.
pub struct ApiFailure {
    /// The HTTP head, normally status 200.
    pub meta: ResponseMeta,
    pub(crate) errors: Vec<String>,
}
impl ApiFailure {
    /// One class per provider error, preserving order and multiplicity.
    pub fn classes(&self) -> Vec<ApiClass> {
        self.errors.iter().map(|s| classify(s)).collect()
    }
    /// Explicit unredacted provider text; ordinary diagnostics withhold it.
    pub fn errors_unredacted(&self) -> &[String] {
        &self.errors
    }
}
impl fmt::Debug for ApiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiFailure")
            .field("meta", &self.meta)
            .field("classes", &self.classes())
            .finish()
    }
}

/// Why a trade-history page cannot continue the offset traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PageProblem {
    /// The page has no `count`, so the end of the walk is unknown.
    MissingCount,
    /// Page length, offset and `count` contradict each other (more than the page
    /// size, beyond `count`, or an empty page before `count`).
    Offset(concord_pagination::OffsetError),
}

/// A decoded trade-history page that the traversal refuses to continue from.
#[derive(Debug)]
pub struct PageFailure {
    /// The page's HTTP head and endpoint identity.
    pub meta: ResponseMeta,
    /// Offset at which the page was requested.
    pub offset: u64,
    /// The inconsistency; no provider value is retained.
    pub problem: PageProblem,
}

/// All SDK failure stages. Nothing is automatically retried.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// A value or provider-independent local rule refused the operation.
    Local(LocalError),
    /// Build, transport, metadata, decode or response-size failure from Concord.
    Concord(concord::Error),
    /// A non-2xx response, keeping the status even when the body failed.
    #[from(skip)]
    Http(Box<HttpFailure>),
    /// Kraken reported at least one error inside a 2xx envelope.
    #[from(skip)]
    Api(Box<ApiFailure>),
    /// An empty error list without a result is not successful.
    #[from(skip)]
    MissingResult(Box<ResponseMeta>),
    /// A trade-history page contradicts the offset walk (see [`PageProblem`]).
    #[from(skip)]
    Page(Box<PageFailure>),
}
impl Error {
    /// Response status when a head arrived (including malformed 2xx responses).
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Local(_) => None,
            Self::Concord(e) => e.head().map(|h| h.status()),
            Self::Http(e) => Some(e.meta.status()),
            Self::Api(e) => Some(e.meta.status()),
            Self::MissingResult(meta) => Some(meta.status()),
            Self::Page(e) => Some(e.meta.status()),
        }
    }
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Local(e) => fmt::Display::fmt(e, f),
        Error::Concord(e) => fmt::Display::fmt(e, f),
        Error::Http(e) => write!(f, "Kraken HTTP failure: {}", e.meta.status().as_u16()),
        Error::Api(e) => write!(f, "Kraken API failure: {:?}", e.classes()),
        Error::MissingResult(_) => f.write_str("Kraken successful envelope is missing its result"),
        Error::Page(e) => write!(f, "Kraken trade-history page refused: {:?}", e.problem),
    }
}
// Untrusted provider/parser/transport sources require explicit access, never an error chain.
