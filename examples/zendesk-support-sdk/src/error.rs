//! The SDK's error type. No `Display` or `Debug` here prints a credential, a URL, a provider
//! description or detail, or a response body.

use std::fmt;
use std::time::{Duration, SystemTime};

use concord::reqwest::StatusCode;
use concord::reqwest::header::RETRY_AFTER;
use concord::{Received, ResponseMeta};
use concord_retry::http::RetryAfterError;
use serde::Deserialize;

use crate::codecs::ProblemFormat;
use crate::{ERROR_BODY_LIMIT, IdempotencyKey};

/// A business-level error document: `{"error": .., "description": .., "details": ..}`.
#[derive(Clone, Deserialize, PartialEq)]
pub struct ErrorDocument {
    error: String,
    description: Option<String>,
    details: Option<serde_json::Value>,
}

impl ErrorDocument {
    /// The provider's error code (`RecordInvalid`, `IdempotentRequestError`, ..).
    pub fn error(&self) -> &str {
        &self.error
    }

    /// The human description. It can echo request content, so it is never formatted by this SDK.
    pub fn description_unredacted(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The per-field details (`{"field": [{"description": .., "error": ..}]}` for
    /// `RecordInvalid`). They can quote submitted values, so they are never formatted.
    pub fn details_unredacted(&self) -> Option<&serde_json::Value> {
        self.details.as_ref()
    }
}

impl fmt::Debug for ErrorDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorDocument")
            .field("error", &self.error)
            .field(
                "description",
                &self.description.as_ref().map(|_| "<withheld>"),
            )
            .field("details", &self.details.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

/// The body of a non-2xx response, as far as it could be read.
#[derive(Clone, PartialEq)]
pub enum Problem {
    /// A JSON error document (business-level errors).
    Document(ErrorDocument),
    /// The JSON document `null` (documented for some 404 and 409 responses).
    Null,
    /// A `text/plain` body (API-level errors).
    Text(String),
}

impl Problem {
    /// The provider's error code, when the body is a JSON error document.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Document(document) => Some(document.error()),
            Self::Null | Self::Text(_) => None,
        }
    }

    /// The `text/plain` body. It can echo anything the request carried; never formatted here.
    pub fn text_unredacted(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Document(_) | Self::Null => None,
        }
    }
}

impl fmt::Debug for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(document) => f.debug_tuple("Document").field(document).finish(),
            Self::Null => f.write_str("Null"),
            Self::Text(text) => write!(f, "Text(<{} bytes withheld>)", text.len()),
        }
    }
}

/// A non-2xx response. The status is authoritative whether or not the body could be read.
#[derive(Debug)]
pub struct HttpError {
    meta: ResponseMeta,
    document: Result<Problem, concord::Error>,
    retry_after: Result<Option<Duration>, RetryAfterError>,
    attempts: usize,
}

impl HttpError {
    pub(crate) async fn read<T>(received: Received<T>, attempts: usize) -> Self {
        let retry_after = concord_retry::http::retry_after(
            received
                .head()
                .headers()
                .get_all(RETRY_AFTER)
                .iter()
                .map(|v| v.as_bytes()),
            SystemTime::now(),
        );
        let (meta, document) = received
            .document::<ProblemFormat, Problem>(ERROR_BODY_LIMIT)
            .await;
        Self {
            meta,
            document,
            retry_after,
            attempts,
        }
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The response head (status, headers, operation); its `Debug` prints header names only.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The error body: a document, `null` or text; or why it could not be read (another media
    /// type, a body over [`ERROR_BODY_LIMIT`], a malformed document, a broken connection).
    pub fn document(&self) -> Result<&Problem, &concord::Error> {
        self.document.as_ref()
    }

    /// Every `Retry-After` line, parsed strictly (seconds or an HTTP date): `Ok(None)` when
    /// absent, `Err` when invalid or repeated. Present on 429 and on some 503 responses.
    pub fn retry_after(&self) -> Result<Option<Duration>, RetryAfterError> {
        self.retry_after
    }

    /// How many times the request was sent: more than one only for a read whose 429/503 carried
    /// a `Retry-After` this SDK honoured.
    pub fn attempts(&self) -> usize {
        self.attempts
    }

    /// `X-Zendesk-Request-Id`, which Zendesk support asks for when reporting a problem.
    pub fn request_id(&self) -> Option<&str> {
        self.meta
            .headers()
            .get("x-zendesk-request-id")
            .and_then(|v| v.to_str().ok())
    }
}

/// Why a write's outcome is unknown.
#[derive(Debug)]
#[non_exhaustive]
pub enum UncertainCause {
    /// The request may have been sent, and no response arrived (a timeout, a connection closed
    /// before the response head).
    NoResponse(concord::Error),
    /// A 5xx response: the server may have committed the write before failing.
    ServerError(Box<HttpError>),
    /// A 2xx response whose head or body could not be read: the write happened, but its result
    /// (the new ticket's id) is unknown.
    UnreadableSuccess(concord::Error),
}

/// A write (Create Ticket) that may or may not have taken effect. The SDK never repeats it.
///
/// Resolve it explicitly: replay the **same** request with the same [`IdempotencyKey`] within
/// two hours (Zendesk answers `x-idempotency-lookup: hit` with the original ticket, or creates it
/// once if the first request never arrived), or look the ticket up by its `external_id`.
#[derive(Debug)]
pub struct UncertainWrite {
    operation: &'static str,
    idempotency_key: Option<IdempotencyKey>,
    cause: UncertainCause,
}

impl UncertainWrite {
    pub(crate) fn new(
        operation: &'static str,
        idempotency_key: Option<IdempotencyKey>,
        cause: UncertainCause,
    ) -> Self {
        Self {
            operation,
            idempotency_key,
            cause,
        }
    }

    /// The declared endpoint (`Zendesk.tickets.create`).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// The `Idempotency-Key` the request carried: replay with it.
    pub fn idempotency_key(&self) -> Option<&IdempotencyKey> {
        self.idempotency_key.as_ref()
    }

    /// What was observed.
    pub fn cause(&self) -> &UncertainCause {
        &self.cause
    }
}

/// Why a cursor traversal could not continue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaginationError {
    /// `meta.has_more` is true but `meta.after_cursor` is missing or empty.
    MissingCursor,
    /// The next cursor equals the one just requested: following it would repeat the page.
    RepeatedCursor,
    /// `links.next` names another `page[after]` than `meta.after_cursor`.
    InconsistentNextLink,
}

/// Errors of this SDK.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A request that could not be built (nothing sent), a transport failure of a read, or a
    /// 2xx read whose head or body was refused (`Metadata`, `Decode`, `Limit`).
    Concord(concord::Error),
    /// A non-2xx response (a 5xx on a write is [`Error::Uncertain`] instead).
    Http(Box<HttpError>),
    /// A write whose outcome is unknown; see [`UncertainWrite`].
    Uncertain(Box<UncertainWrite>),
    /// The OAuth token is not a valid bearer credential (empty, whitespace, controls, non-ASCII).
    Credential(concord_auth::CredentialError),
    /// An SDK input outside its documented range; nothing was sent.
    InvalidInput {
        /// Which input.
        what: &'static str,
    },
    /// A cursor page that cannot be followed safely.
    Pagination(PaginationError),
    /// The read deadline of [`SendPolicy::read_deadline`](crate::SendPolicy::read_deadline)
    /// expired before any response arrived.
    DeadlineExceeded,
}

impl Error {
    /// The HTTP status of an [`Error::Http`] (or of the 5xx behind an uncertain write).
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Http(http) => Some(http.status()),
            Self::Uncertain(write) => match &write.cause {
                UncertainCause::ServerError(http) => Some(http.status()),
                _ => None,
            },
            _ => None,
        }
    }

    /// The HTTP error, if this is one.
    pub fn http(&self) -> Option<&HttpError> {
        match self {
            Self::Http(http) => Some(http),
            _ => None,
        }
    }

    /// The uncertain write, if this is one.
    pub fn uncertain(&self) -> Option<&UncertainWrite> {
        match self {
            Self::Uncertain(write) => Some(write),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(value: concord::Error) -> Self {
        Self::Concord(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => e.fmt(f),
            Self::Http(http) => {
                write!(f, "Zendesk responded {}", http.status())?;
                if let Ok(Some(code)) = http.document().map(Problem::error) {
                    write!(f, " ({code})")?;
                }
                Ok(())
            }
            Self::Uncertain(write) => {
                write!(f, "the outcome of `{}` is unknown: ", write.operation)?;
                f.write_str(match write.cause {
                    UncertainCause::NoResponse(_) => "no response arrived",
                    UncertainCause::ServerError(_) => "the server failed with a 5xx",
                    UncertainCause::UnreadableSuccess(_) => "the success response was unreadable",
                })?;
                f.write_str("; replay it with the same idempotency key, never with a new one")
            }
            Self::Credential(e) => write!(f, "invalid OAuth bearer token: {e}"),
            Self::InvalidInput { what } => write!(f, "invalid {what}"),
            Self::Pagination(kind) => f.write_str(match kind {
                PaginationError::MissingCursor => "a page with more results has no after cursor",
                PaginationError::RepeatedCursor => "a page names the cursor that produced it",
                PaginationError::InconsistentNextLink => {
                    "a page's next link and after cursor disagree"
                }
            }),
            Self::DeadlineExceeded => f.write_str("the read deadline expired before a response"),
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
