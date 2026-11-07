//! The SDK's error: provider statuses with their bounded error document, Concord failures, and
//! 2xx pages that contradict the documented contract.

use std::fmt;

use concord::reqwest::StatusCode;
use concord::{ErrorKind, Operation};
use concord_pagination::OffsetError;
use serde::Deserialize;

/// Owned response metadata (status, HTTP version, headers, operation), Concord's own type.
/// `Debug` prints header names only.
pub use concord::ResponseMeta;

/// The provider's error document: `{"errors": [{"message": ".."}]}`. Unknown keys are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ErrorDocument {
    /// The reported failures, in order.
    pub errors: Vec<ErrorEntry>,
}

/// One entry of [`ErrorDocument::errors`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ErrorEntry {
    /// What went wrong. Untrusted provider text: read it deliberately.
    pub message: String,
}

/// The documented meaning of a non-2xx status (Countries API "Errors"). The status itself, in
/// [`ProviderError::status`], stays authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderClass {
    /// `400`: a malformed request (an empty `q`, a `limit` out of range, another parameter).
    BadRequest,
    /// `401`: a missing, unrecognized, revoked or expired API key.
    Unauthorized,
    /// `403`: the account is frozen (monthly limit reached), or a free plan searched, filtered
    /// or looked up by a premium property such as `leaders`. The document's message says which.
    Forbidden,
    /// `404`: an unknown path, or a property or aggregate that is not readable or searchable.
    NotFound,
    /// `405`: a method other than `GET`.
    MethodNotAllowed,
    /// `410`: the API version is no longer active.
    Gone,
    /// `429`: the edge rate limiter (sustained traffic above 20 requests per 10 seconds).
    RateLimited,
    /// `500` (and any other 5xx): a server failure.
    Server,
    /// Any other status, not documented by the provider.
    Undocumented,
}

impl ProviderClass {
    /// The class of `status`.
    pub fn of(status: StatusCode) -> Self {
        match status.as_u16() {
            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            405 => Self::MethodNotAllowed,
            410 => Self::Gone,
            429 => Self::RateLimited,
            500..=599 => Self::Server,
            _ => Self::Undocumented,
        }
    }
}

/// Why a non-2xx response's error document could not be read. Each variant holds the
/// [`concord::Error`] of `Received::document`, with its kind, operation and response head.
#[derive(Debug)]
pub enum ErrorBodyError {
    /// Missing, repeated or non-JSON `Content-Type` (a proxy's HTML page); the body was not
    /// read (kind `Metadata`).
    Metadata(concord::Error),
    /// The body exceeded [`crate::ERROR_BODY_LIMIT`] or could not be read completely (kind
    /// `Limit` or `Transport`).
    Read(concord::Error),
    /// JSON-labelled, but not an [`ErrorDocument`] (kind `Decode`).
    Decode(concord::Error),
}

impl From<concord::Error> for ErrorBodyError {
    fn from(error: concord::Error) -> Self {
        match error.kind() {
            ErrorKind::Metadata => Self::Metadata(error),
            ErrorKind::Decode => Self::Decode(error),
            _ => Self::Read(error),
        }
    }
}

impl ErrorBodyError {
    fn source_error(&self) -> &concord::Error {
        match self {
            Self::Metadata(error) | Self::Read(error) | Self::Decode(error) => error,
        }
    }
}

impl fmt::Display for ErrorBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.source_error(), f)
    }
}

impl std::error::Error for ErrorBodyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source_error())
    }
}

/// A non-2xx answer: the response head and the bounded attempt to read the error document.
///
/// `Debug` and `Display` show the status and class, never the provider's messages (they are
/// untrusted text); read them with [`messages`](Self::messages).
pub struct ProviderError {
    /// The response head, kept whatever happened to the document.
    pub meta: ResponseMeta,
    /// The typed document, or why it could not be read.
    pub document: Result<ErrorDocument, ErrorBodyError>,
}

impl ProviderError {
    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The documented meaning of the status.
    pub fn class(&self) -> ProviderClass {
        ProviderClass::of(self.status())
    }

    /// The document's messages, in order (none when the document could not be read).
    pub fn messages(&self) -> impl Iterator<Item = &str> {
        self.document
            .iter()
            .flat_map(|document| document.errors.iter().map(|entry| entry.message.as_str()))
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("meta", &self.meta)
            .field("class", &self.class())
            .field(
                "messages",
                &self.document.as_ref().map(|d| d.errors.len()).ok(),
            )
            .field("document_error", &self.document.as_ref().err())
            .finish()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "REST Countries answered HTTP {}", self.status())
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.document.as_ref().err().map(|error| error as _)
    }
}

/// A 2xx page that contradicts the documented page or lookup contract. No value of the page is
/// shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContractError {
    /// `meta.count` (or `meta.total`, for a single lookup) differs from the records received.
    CountMismatch,
    /// The page arithmetic is impossible (more records than `limit`, past `total`, an empty
    /// page before `total`).
    Offset(OffsetError),
    /// `meta.more` contradicts `offset + count` against `total`.
    MoreMismatch,
    /// A lookup expected to be unique (an ISO code) matched several records.
    NotUnique {
        /// The number of matches the provider reported.
        total: u64,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountMismatch => f.write_str("the page's count disagrees with its records"),
            Self::Offset(error) => write!(f, "inconsistent page: {error}"),
            Self::MoreMismatch => f.write_str("the page's `more` flag contradicts its offsets"),
            Self::NotUnique { total } => write!(f, "a unique lookup matched {total} records"),
        }
    }
}

impl std::error::Error for ContractError {}

/// What every generated `send()` (and [`crate::pages`]) returns on failure.
///
/// `with_base_url` and `prepare()` return [`concord::Error`] instead (rustdoc shows both as
/// `Error`); `From<concord::Error>` converts it into [`Error::Concord`], so `?` works.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Any non-2xx status, with the bounded error-document attempt. A no-match is *not* this:
    /// it is a successful, empty [`CountryPage`](crate::CountryPage).
    Provider(Box<ProviderError>),
    /// Building, sending, the success `Content-Type`, reading (`Limit`, `Transport`) or decoding
    /// the success body. After a head arrived, `error.head()` keeps it.
    Concord(concord::Error),
    /// A 2xx page that contradicts the contract ([`ContractError`]).
    Contract(ContractError),
}

impl Error {
    /// The HTTP status, when a response head arrived.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Provider(error) => Some(error.status()),
            Self::Concord(error) => error.head().map(|head| head.status()),
            Self::Contract(_) => None,
        }
    }

    /// The provider error, for a non-2xx status.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Provider(error) => Some(error),
            _ => None,
        }
    }

    /// The generated operation (`RestCountries.countries`, ...), when known.
    pub fn operation(&self) -> Option<Operation> {
        match self {
            Self::Provider(error) => error.meta.operation(),
            Self::Concord(error) => error.operation(),
            Self::Contract(_) => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(error)
    }
}

impl From<ContractError> for Error {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(error) => fmt::Display::fmt(error, f),
            Self::Concord(error) => fmt::Display::fmt(error, f),
            Self::Contract(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::Concord(error) => Some(error),
            Self::Contract(error) => Some(error),
        }
    }
}
