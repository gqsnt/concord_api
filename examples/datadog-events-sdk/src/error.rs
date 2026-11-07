//! The SDK's errors. None of them contains or prints a key: `Display` and `Debug` show the
//! stage, the status, the site and counts; provider text is read deliberately
//! ([`ProviderError::messages`]).

use std::fmt;

use concord::reqwest::StatusCode;
use concord::{ErrorKind, Operation, ResponseMeta};
use serde::Deserialize;

use crate::site::Site;

/// The documented error body of every non-2xx answer: `{"errors": ["..."]}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ErrorDocument {
    /// "A list of errors" (required by the schema). Untrusted provider text.
    pub errors: Vec<String>,
}

/// Why the error document of a non-2xx answer could not be read. Each variant holds the
/// [`concord::Error`] of `Received::document` (its kind, operation and response head). The
/// status of the answer stays authoritative either way.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorBodyError {
    /// The body is not labelled JSON (or not labelled); it was not read (kind `Metadata`).
    Metadata(concord::Error),
    /// The body exceeded [`ERROR_BODY_LIMIT`](crate::ERROR_BODY_LIMIT) or broke off (kind
    /// `Limit` or `Transport`).
    Read(concord::Error),
    /// The body is labelled JSON but is not an `{"errors": [..]}` document (kind `Decode`).
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
    /// The underlying Concord error.
    pub fn concord(&self) -> &concord::Error {
        match self {
            Self::Metadata(error) | Self::Read(error) | Self::Decode(error) => error,
        }
    }
}

impl fmt::Display for ErrorBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.concord(), f)
    }
}

impl std::error::Error for ErrorBodyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.concord())
    }
}

/// A non-2xx answer: its head and its error document (or why there is none).
pub struct ProviderError {
    pub(crate) meta: ResponseMeta,
    pub(crate) document: Result<ErrorDocument, ErrorBodyError>,
}

impl ProviderError {
    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The response head (status, version, headers, operation). Header values are not
    /// redacted here: reading them is a deliberate choice.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The decoded error document, or why it could not be read.
    pub fn document(&self) -> Result<&ErrorDocument, &ErrorBodyError> {
        self.document.as_ref()
    }

    /// The provider's error messages, when the document decoded. Untrusted text: neither
    /// `Display` nor `Debug` of this crate prints it.
    pub fn messages(&self) -> Option<&[String]> {
        self.document.as_ref().ok().map(|doc| doc.errors.as_slice())
    }

    fn describe(&self) -> String {
        match &self.document {
            Ok(doc) => format!("{} provider message(s)", doc.errors.len()),
            Err(ErrorBodyError::Metadata(_)) => "error document not JSON".to_owned(),
            Err(ErrorBodyError::Read(_)) => "error document unreadable".to_owned(),
            Err(ErrorBodyError::Decode(_)) => "error document malformed".to_owned(),
        }
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("meta", &self.meta)
            .field("messages", &self.messages().map(<[String]>::len))
            .field("document_error", &self.document.as_ref().err())
            .finish()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {} ({})", self.status(), self.describe())
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.document.as_ref().err().map(|error| error as _)
    }
}

/// Which of the two keys an [`AuthError`] concerns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The API key (`DD-API-KEY`).
    Api,
    /// The application key (`DD-APPLICATION-KEY`).
    Application,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Key::Api => "API key",
            Key::Application => "application key",
        })
    }
}

/// Datadog answered 401 or 403: the keys are wrong, revoked, lack the `events_read`
/// permission, or belong to another site.
#[derive(Debug)]
pub struct Rejected {
    pub(crate) host: String,
    pub(crate) site: Option<Site>,
    pub(crate) provider: ProviderError,
}

impl Rejected {
    /// The host that answered (the request's, since redirects are not followed).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The listed site of that host; `None` for another host (a proxy, a stand-in).
    pub fn site(&self) -> Option<Site> {
        self.site
    }

    /// The answer: status, head and error document.
    pub fn provider(&self) -> &ProviderError {
        &self.provider
    }
}

/// An authentication failure: a key that is missing or cannot be sent (nothing was sent), or
/// a site that rejected the keys.
#[derive(Debug)]
#[non_exhaustive]
pub enum AuthError {
    /// The API key is empty. Nothing was sent.
    MissingApiKey,
    /// The application key is empty: reads require it. Nothing was sent.
    MissingApplicationKey,
    /// A key holds bytes an HTTP header cannot carry. Nothing was sent.
    MalformedKey(Key),
    /// The site answered 401 or 403.
    Rejected(Box<Rejected>),
}

impl AuthError {
    /// Whether the failure happened before anything was sent.
    pub fn is_local(&self) -> bool {
        !matches!(self, AuthError::Rejected(_))
    }
}

impl PartialEq for AuthError {
    /// Local failures compare by kind; two rejections compare by host and status.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MissingApiKey, Self::MissingApiKey)
            | (Self::MissingApplicationKey, Self::MissingApplicationKey) => true,
            (Self::MalformedKey(a), Self::MalformedKey(b)) => a == b,
            (Self::Rejected(a), Self::Rejected(b)) => {
                a.host == b.host && a.provider.status() == b.provider.status()
            }
            _ => false,
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => f.write_str("Datadog API key is empty; nothing was sent"),
            Self::MissingApplicationKey => f.write_str(
                "Datadog application key is empty: read requests need an API key and an \
                 application key; nothing was sent",
            ),
            Self::MalformedKey(key) => write!(
                f,
                "Datadog {key} holds bytes an HTTP header cannot carry; nothing was sent"
            ),
            Self::Rejected(rejected) => {
                match rejected.site {
                    Some(site) => write!(f, "Datadog site {site}")?,
                    None => write!(f, "Datadog API host {}", rejected.host)?,
                }
                write!(
                    f,
                    " rejected the credentials: {}; keys belong to one Datadog site, so check \
                     the configured site and both keys",
                    rejected.provider
                )
            }
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected(rejected) => Some(&rejected.provider),
            _ => None,
        }
    }
}

/// A 2xx page that breaks the documented pagination contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProtocolError {
    /// `meta.page.after` is present but empty: sending it back would not continue the
    /// listing.
    EmptyCursor,
    /// `meta.page.after` names a cursor this traversal already requested: following it
    /// would loop.
    RepeatedCursor,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyCursor => "the page's `meta.page.after` cursor is empty",
            Self::RepeatedCursor => {
                "the page's `meta.page.after` cursor was already requested by this traversal"
            }
        })
    }
}

impl std::error::Error for ProtocolError {}

/// Every failure of a request sent through this SDK (each generated `send()` returns it).
///
/// `prepare()` and `with_base_url` return [`concord::Error`] instead; `?` converts it into
/// [`Error::Concord`].
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The keys are missing, malformed or rejected (401/403), the site included.
    Authentication(AuthError),
    /// 400: the provider refused the request (a query it cannot parse, say).
    BadRequest(Box<ProviderError>),
    /// 429: too many requests. The head is kept (`ProviderError::meta`); the SDK does not
    /// retry.
    RateLimited(Box<ProviderError>),
    /// Any other non-2xx status (including a redirect, which the SDK's transport does not
    /// follow).
    Provider(Box<ProviderError>),
    /// A 2xx page broke the pagination contract.
    Protocol(ProtocolError),
    /// Build, transport, response media type, size limit or success-body decoding: Concord's
    /// error, with the response head when one arrived.
    Concord {
        /// Concord's error (kind, operation, head).
        source: concord::Error,
        /// The response head, when one arrived.
        meta: Option<Box<ResponseMeta>>,
    },
}

impl Error {
    /// Whether this is an authentication failure (local or a 401/403).
    pub fn is_authentication(&self) -> bool {
        matches!(self, Error::Authentication(_))
    }

    /// The HTTP status of the answer, when one arrived.
    pub fn status(&self) -> Option<StatusCode> {
        self.meta().map(ResponseMeta::status)
    }

    /// The response head, when one arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Error::Authentication(AuthError::Rejected(rejected)) => Some(&rejected.provider.meta),
            Error::BadRequest(provider)
            | Error::RateLimited(provider)
            | Error::Provider(provider) => Some(&provider.meta),
            Error::Concord { meta, .. } => meta.as_deref(),
            _ => None,
        }
    }

    /// The provider answer of a non-2xx status.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Error::Authentication(AuthError::Rejected(rejected)) => Some(&rejected.provider),
            Error::BadRequest(provider)
            | Error::RateLimited(provider)
            | Error::Provider(provider) => Some(provider),
            _ => None,
        }
    }

    /// The operation that failed (`DatadogApi.events.list`), when known.
    pub fn operation(&self) -> Option<Operation> {
        match self {
            Error::Concord { source, .. } => source.operation(),
            other => other.meta().and_then(ResponseMeta::operation),
        }
    }
}

impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta().map(Box::new);
        Error::Concord { source, meta }
    }
}

impl From<AuthError> for Error {
    fn from(error: AuthError) -> Self {
        Error::Authentication(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Authentication(error) => fmt::Display::fmt(error, f),
            Error::BadRequest(provider) => write!(f, "Datadog refused the request: {provider}"),
            Error::RateLimited(provider) => write!(f, "Datadog rate limit reached: {provider}"),
            Error::Provider(provider) => write!(f, "Datadog answered {provider}"),
            Error::Protocol(error) => write!(f, "Datadog pagination contract broken: {error}"),
            Error::Concord { source, .. } => fmt::Display::fmt(source, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Authentication(error) => Some(error),
            Error::BadRequest(provider)
            | Error::RateLimited(provider)
            | Error::Provider(provider) => Some(provider.as_ref()),
            Error::Protocol(error) => Some(error),
            Error::Concord { source, .. } => Some(source),
        }
    }
}
