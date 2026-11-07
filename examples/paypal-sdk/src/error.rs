//! The SDK's error type. No `Display` or `Debug` here prints a credential, an access token, a
//! URL, a provider message or description, or a body.

use std::fmt;
use std::time::{Duration, SystemTime};

use concord::reqwest::StatusCode;
use concord::reqwest::header::RETRY_AFTER;
use concord::{Json, Received, ResponseMeta};
use concord_retry::http::RetryAfterError;
use serde::Deserialize;

use crate::{ERROR_BODY_LIMIT, Link, RequestId, TokenError};

/// A PayPal error document: `name`, `message`, `debug_id`, `details`, `links`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ApiError {
    name: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    debug_id: Option<String>,
    #[serde(default)]
    details: Vec<ErrorDetail>,
    #[serde(default)]
    links: Vec<Link>,
}

impl ApiError {
    /// The error name (`UNPROCESSABLE_ENTITY`, `RESOURCE_NOT_FOUND`, `RATE_LIMIT_REACHED`, ...).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// PayPal's identifier of this failure, for its support.
    pub fn debug_id(&self) -> Option<&str> {
        self.debug_id.as_deref()
    }

    /// The details, each with its `issue` code (`ORDER_NOT_APPROVED`, ...).
    pub fn details(&self) -> &[ErrorDetail] {
        &self.details
    }

    /// The issue codes of the details, in order.
    pub fn issues(&self) -> impl Iterator<Item = &str> {
        self.details.iter().filter_map(|d| d.issue.as_deref())
    }

    /// The human message. It can echo request content, so it is never formatted by this SDK.
    pub fn message_unredacted(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Links to documentation of this error. Never followed by this SDK.
    pub fn links(&self) -> &[Link] {
        &self.links
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("name", &self.name)
            .field("debug_id", &self.debug_id)
            .field("issues", &self.issues().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

/// One entry of an error document's `details`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ErrorDetail {
    #[serde(default)]
    issue: Option<String>,
    #[serde(default)]
    field: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl ErrorDetail {
    /// The issue code (`ORDER_NOT_APPROVED`, `PREVIOUS_REQUEST_IN_PROGRESS`, ...).
    pub fn issue(&self) -> Option<&str> {
        self.issue.as_deref()
    }

    /// The JSON pointer of the field concerned, when given.
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Where the field is (`body`, `path`, `query`, `header`), when given.
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    /// The offending value as PayPal echoes it. Never formatted by this SDK.
    pub fn value_unredacted(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// The human description. Never formatted by this SDK.
    pub fn description_unredacted(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

impl fmt::Debug for ErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorDetail")
            .field("issue", &self.issue)
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}

/// A non-2xx response. The status is authoritative whether or not its document could be read.
#[derive(Debug)]
pub struct HttpError {
    operation: &'static str,
    meta: ResponseMeta,
    document: Result<ApiError, concord::Error>,
    retry_after: Result<Option<Duration>, RetryAfterError>,
    attempts: usize,
}

impl HttpError {
    pub(crate) async fn read<T>(
        received: Received<T>,
        operation: &'static str,
        attempts: usize,
    ) -> Self {
        let retry_after = concord_retry::http::retry_after(
            received
                .head()
                .headers()
                .get_all(RETRY_AFTER)
                .iter()
                .map(|v| v.as_bytes()),
            SystemTime::now(),
        );
        let (meta, document) = received.document::<Json, ApiError>(ERROR_BODY_LIMIT).await;
        Self {
            operation,
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

    /// The response head (`Debug` shows header names only).
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The error document, or why it could not be read (wrong media type, too long, not JSON).
    pub fn document(&self) -> Result<&ApiError, &concord::Error> {
        self.document.as_ref()
    }

    /// The document's error name, when readable.
    pub fn name(&self) -> Option<&str> {
        self.document.as_ref().ok().map(ApiError::name)
    }

    /// Whether any detail has this issue code.
    pub fn has_issue(&self, issue: &str) -> bool {
        self.document
            .as_ref()
            .is_ok_and(|d| d.issues().any(|i| i == issue))
    }

    /// A `429` (PayPal: `RATE_LIMIT_REACHED`). PayPal publishes no quota and documents no
    /// `Retry-After`; this SDK never repeats it.
    pub fn is_rate_limited(&self) -> bool {
        self.status() == StatusCode::TOO_MANY_REQUESTS
    }

    /// A `Retry-After` the response carried (PayPal does not document one): absent is
    /// `Ok(None)`; repeated or invalid is an error.
    pub fn retry_after(&self) -> Result<Option<Duration>, RetryAfterError> {
        self.retry_after
    }

    /// Attempts made for this call, this response's included.
    pub fn attempts(&self) -> usize {
        self.attempts
    }

    /// The endpoint (`PayPal.order.capture`).
    pub fn operation(&self) -> &'static str {
        self.operation
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} answered HTTP {}",
            self.operation,
            self.status().as_u16()
        )?;
        match &self.document {
            Ok(document) => {
                write!(f, " {}", document.name)?;
                let issues: Vec<&str> = document.issues().collect();
                if !issues.is_empty() {
                    write!(f, " ({})", issues.join(", "))?;
                }
                if let Some(id) = &document.debug_id {
                    write!(f, ", debug_id {id}")?;
                }
                Ok(())
            }
            Err(e) => write!(f, " (error document unreadable: {:?})", e.kind()),
        }
    }
}

/// What was last observed of a write that may have been applied.
#[derive(Debug)]
pub enum UncertainCause {
    /// No response arrived after the request was handed to the client (a timeout, a reset).
    NoResponse(concord::Error),
    /// PayPal answered `5xx`: the write may have been performed.
    ServerError(Box<HttpError>),
    /// PayPal answered `2xx`, but the body could not be read or decoded: the write happened and
    /// its result is unknown.
    UnreadableSuccess(concord::Error),
    /// A later attempt, after an uncertain one, was refused (`409`, `422`, `429`, ...). The
    /// refusal does not say whether the earlier attempt was performed.
    LaterRefusal(Box<HttpError>),
    /// A later attempt, after an uncertain one, could not obtain an access token.
    LaterToken(TokenError),
    /// The overall deadline cancelled an attempt in flight.
    Interrupted,
}

/// A write (create, authorize, capture or void) that may have been applied. Never repeated
/// automatically beyond the session's bounds.
#[derive(Debug)]
pub struct Uncertain {
    pub(crate) operation: &'static str,
    pub(crate) request_id: Option<RequestId>,
    pub(crate) attempts: usize,
    pub(crate) cause: UncertainCause,
}

impl Uncertain {
    /// The endpoint (`PayPal.orders.create`, `PayPal.order.authorize`,
    /// `PayPal.order.capture`, `PayPal.authorization.capture` or `PayPal.authorization.void`).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// The `PayPal-Request-Id` every attempt carried. Sending the same call again with it, within
    /// PayPal's retention (6 hours by default), returns the stored outcome instead of performing
    /// the write twice. `None`: the write was sent once without one; read the order's state (Show
    /// order details) before deciding to send it again.
    pub fn request_id(&self) -> Option<&RequestId> {
        self.request_id.as_ref()
    }

    /// Attempts started for this call.
    pub fn attempts(&self) -> usize {
        self.attempts
    }

    /// The last observation.
    pub fn cause(&self) -> &UncertainCause {
        &self.cause
    }
}

/// An SDK failure.
#[derive(Debug)]
pub enum Error {
    /// An input refused before any request was built.
    Invalid(&'static str),
    /// A request that could not be built (nothing sent), or a read that failed without a
    /// usable answer: transport, response head refused (`Metadata`), body too long (`Limit`),
    /// body not decodable (`Decode`).
    Concord(concord::Error),
    /// No access token could be obtained; the API request was not sent.
    Token(TokenError),
    /// PayPal answered with a non-2xx status: a read failed, or a write was refused (`4xx`).
    Http(Box<HttpError>),
    /// A create, authorize, capture or void may have been applied.
    Uncertain(Box<Uncertain>),
    /// The request's origin is not the one the session's credentials are bound to (the client
    /// was moved with `with_base_url`); nothing was sent and no token was requested.
    Destination,
    /// The overall read deadline elapsed before any attempt completed.
    Deadline,
}

impl Error {
    /// The status of an [`Error::Http`].
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Http(http) => Some(http.status()),
            _ => None,
        }
    }

    /// The [`Uncertain`] write, if this is one.
    pub fn uncertain(&self) -> Option<&Uncertain> {
        match self {
            Self::Uncertain(uncertain) => Some(uncertain),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(why) => write!(f, "PayPal: invalid input: {why}"),
            Self::Concord(e) => write!(f, "PayPal request failed: {e}"),
            Self::Token(e) => write!(f, "PayPal access token: {e}"),
            Self::Http(http) => write!(f, "PayPal: {http}"),
            Self::Uncertain(u) => {
                write!(
                    f,
                    "PayPal: {} may have been applied after {} attempt(s) (",
                    u.operation, u.attempts
                )?;
                match &u.cause {
                    UncertainCause::NoResponse(e) => write!(f, "no response: {:?}", e.kind())?,
                    UncertainCause::ServerError(h) => write!(f, "HTTP {}", h.status().as_u16())?,
                    UncertainCause::UnreadableSuccess(e) => {
                        write!(f, "unreadable success: {:?}", e.kind())?;
                    }
                    UncertainCause::LaterRefusal(h) => {
                        write!(f, "then refused with HTTP {}", h.status().as_u16())?;
                    }
                    UncertainCause::LaterToken(_) => f.write_str("then no access token")?,
                    UncertainCause::Interrupted => f.write_str("attempt cancelled by deadline")?,
                }
                if u.request_id.is_some() {
                    f.write_str("); resend with the same PayPal-Request-Id to learn the outcome")
                } else {
                    f.write_str("); no PayPal-Request-Id: read the order before resending")
                }
            }
            Self::Destination => {
                f.write_str("PayPal: request origin differs from the session's; nothing was sent")
            }
            Self::Deadline => f.write_str("PayPal: deadline elapsed before an attempt completed"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Concord(e) => Some(e),
            Self::Token(e) => Some(e),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(e)
    }
}
