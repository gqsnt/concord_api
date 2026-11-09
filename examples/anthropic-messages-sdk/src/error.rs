//! The SDK's error type: local refusals, the API's error documents, Concord's failures and
//! incomplete streams.
//!
//! `Display` and `Debug` show statuses, error types, the `request-id` and counts. They never show
//! the API key (Concord withholds it), other header values, the server's error `message`, any
//! generated text or a parser error that may quote it: those are reachable through accessors
//! ([`ApiError::message`], [`ApiError::meta`], [`Incomplete::partial`],
//! [`Incomplete::invalid_event_source`]).

use std::fmt;
use std::time::{Duration, SystemTime};

use concord::reqwest::StatusCode;
use concord::reqwest::header::HeaderMap;
use concord::stream::ReadError;
use concord::{Document, Failure, ResponseMeta, field};
use concord_retry::http::RetryAfterError;
use serde::{Deserialize, Deserializer};

use crate::model::InvalidRequest;
use crate::stream::PartialMessage;

/// Everything a call of this SDK can fail with.
#[derive(Debug, concord::derive::Error)]
#[non_exhaustive]
pub enum Error {
    /// The request was refused by this SDK before anything was sent.
    #[display("invalid message request: {0}")]
    InvalidRequest(#[source] InvalidRequest),
    /// The API answered with a non-2xx status: its error document, or why it could not be read.
    /// This covers errors of the streaming endpoint **before** its stream started.
    #[display("{0}")]
    #[from(concord::Error<ErrorDocument>)]
    Api(Box<ApiError>),
    /// Concord failed: the request could not be built (`Build`, nothing sent), no response
    /// arrived (`Transport`), or a 2xx response was refused (`Metadata`: not
    /// `application/json`, or not `text/event-stream` for a stream), unreadable (`Transport`
    /// mid-body), too long (`Limit`) or not the declared count/message (`Decode`). After a head
    /// arrived, `error.head()` keeps it.
    #[display("{0}")]
    Concord(#[source] concord::Error),
    /// A stream ended before `message_stop`: an `error` event, the end of the body, a broken
    /// connection, malformed events or an event order the documentation does not allow. The
    /// message is **not** complete; nothing was re-sent.
    #[display("{0}")]
    Incomplete(Box<Incomplete>),
}

impl Error {
    /// The `request-id` the API attached to the response this error concerns, if one arrived:
    /// the header of an error response (or the document's `request_id`), the head a Concord
    /// error kept, or the head of the stream that ended early.
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::InvalidRequest(_) => None,
            Self::Api(api) => api.request_id(),
            Self::Concord(error) => error.head().and_then(|head| request_id(head.headers())),
            Self::Incomplete(incomplete) => incomplete.request_id(),
        }
    }

    /// The HTTP status of the response this error concerns: an error response's, or the head a
    /// Concord error kept (a refused or unreadable 2xx). `None` when no response arrived, and for
    /// an incomplete stream, whose failure is in the body of a 2xx.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::InvalidRequest(_) | Self::Incomplete(_) => None,
            Self::Api(api) => Some(api.status()),
            Self::Concord(error) => error.head().map(|head| head.status()),
        }
    }
}

/// The `request-id` header: one field line of text, otherwise none (absent, repeated or not
/// text are not told apart: it is a support identifier, not a protocol input).
pub(crate) fn request_id(headers: &HeaderMap) -> Option<&str> {
    field::single(headers, "request-id")
        .ok()
        .flatten()
        .and_then(|value| value.to_str().ok())
}

/// The `type` of an API error (the errors page's list); a value added later is kept verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ApiErrorType {
    /// `invalid_request_error` (400, and other 4xx).
    InvalidRequest,
    /// `authentication_error` (401).
    Authentication,
    /// `billing_error` (402).
    Billing,
    /// `permission_error` (403).
    Permission,
    /// `not_found_error` (404).
    NotFound,
    /// `conflict_error` (409).
    Conflict,
    /// `request_too_large` (413).
    RequestTooLarge,
    /// `rate_limit_error` (429).
    RateLimit,
    /// `api_error` (500).
    Api,
    /// `timeout_error` (504, also with other statuses such as 500).
    Timeout,
    /// `overloaded_error` (529; also sent as a stream `error` event).
    Overloaded,
    /// Another `type`, verbatim.
    Other(String),
}

impl ApiErrorType {
    /// The wire value.
    pub fn as_str(&self) -> &str {
        match self {
            Self::InvalidRequest => "invalid_request_error",
            Self::Authentication => "authentication_error",
            Self::Billing => "billing_error",
            Self::Permission => "permission_error",
            Self::NotFound => "not_found_error",
            Self::Conflict => "conflict_error",
            Self::RequestTooLarge => "request_too_large",
            Self::RateLimit => "rate_limit_error",
            Self::Api => "api_error",
            Self::Timeout => "timeout_error",
            Self::Overloaded => "overloaded_error",
            Self::Other(other) => other,
        }
    }

    fn from_wire(value: String) -> Self {
        match value.as_str() {
            "invalid_request_error" => Self::InvalidRequest,
            "authentication_error" => Self::Authentication,
            "billing_error" => Self::Billing,
            "permission_error" => Self::Permission,
            "not_found_error" => Self::NotFound,
            "conflict_error" => Self::Conflict,
            "request_too_large" => Self::RequestTooLarge,
            "rate_limit_error" => Self::RateLimit,
            "api_error" => Self::Api,
            "timeout_error" => Self::Timeout,
            "overloaded_error" => Self::Overloaded,
            _ => Self::Other(value),
        }
    }
}

impl<'de> Deserialize<'de> for ApiErrorType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from_wire)
    }
}

impl fmt::Display for ApiErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The `error` object of an error document or of a stream `error` event: `type` and `message`.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderError {
    #[serde(rename = "type")]
    error_type: ApiErrorType,
    message: String,
}

impl ProviderError {
    /// The error type.
    pub fn error_type(&self) -> &ApiErrorType {
        &self.error_type
    }

    /// The server's message, verbatim (not shown by `Display` or `Debug`).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("error_type", &self.error_type)
            .field("message", &format_args!("<{} bytes>", self.message.len()))
            .finish()
    }
}

/// The error document: `{"type": "error", "error": {...}, "request_id": "req_..."}`.
#[derive(Clone, Deserialize)]
pub struct ErrorDocument {
    error: ProviderError,
    #[serde(default)]
    request_id: Option<String>,
}

/// A non-2xx answer. The status is authoritative: it is kept whether the error document is
/// typed, malformed, of another media type, truncated or too long ([`failure`]).
///
/// [`failure`]: Self::failure
pub struct ApiError {
    failure: Failure<ErrorDocument>,
}

impl From<Failure<ErrorDocument>> for ApiError {
    fn from(failure: Failure<ErrorDocument>) -> Self {
        Self { failure }
    }
}

impl ApiError {
    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.failure.status()
    }

    /// The document's `error.type`, when the document was read.
    pub fn error_type(&self) -> Option<&ApiErrorType> {
        self.provider_error().map(ProviderError::error_type)
    }

    /// The document's `error.message`, verbatim, when the document was read.
    pub fn message(&self) -> Option<&str> {
        self.provider_error().map(ProviderError::message)
    }

    /// The document's `error` object, when the document was read.
    pub fn provider_error(&self) -> Option<&ProviderError> {
        self.failure.document().map(|document| &document.error)
    }

    /// The `request-id` header, or else the document's `request_id` field.
    pub fn request_id(&self) -> Option<&str> {
        request_id(self.meta().headers()).or_else(|| {
            self.failure
                .document()
                .and_then(|document| document.request_id.as_deref())
        })
    }

    /// The error document's outcome and bytes: `Unrecognized` (not `application/json`, or not
    /// the documented shape), `TooLong` (longer than [`ERROR_BODY_LIMIT`]), `Broken` (the body
    /// broke off) or `Empty`; `Read` when it was read.
    ///
    /// [`ERROR_BODY_LIMIT`]: crate::ERROR_BODY_LIMIT
    pub fn failure(&self) -> &Failure<ErrorDocument> {
        &self.failure
    }

    /// The response head (status, headers, operation), as received.
    pub fn meta(&self) -> &ResponseMeta {
        self.failure.meta()
    }

    /// The server's `retry-after` delay, judged at `now` (an HTTP date is relative to it):
    /// `Ok(None)` when absent — the errors page notes that a spend-cap 429 has none and keeps
    /// failing — and an error when invalid or repeated. This SDK never retries by itself; the
    /// caller decides whether a request may be sent again.
    ///
    /// # Errors
    ///
    /// [`RetryAfterError`] for an invalid, repeated or overflowing field.
    pub fn retry_after(&self, now: SystemTime) -> Result<Option<Duration>, RetryAfterError> {
        let lines = self.meta().headers().get_all("retry-after");
        concord_retry::http::retry_after(lines.iter().map(|value| value.as_bytes()), now)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the Anthropic API answered {}", self.status().as_u16())?;
        match self.failure.state() {
            Document::Read(document) => write!(f, " ({})", document.error.error_type)?,
            state => write!(f, " (error document unreadable: {state:?})")?,
        }
        if let Some(id) = self.request_id() {
            write!(f, ", request {id}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("status", &self.status().as_u16())
            .field("error", &self.provider_error())
            .field("document", self.failure.state())
            .field("request_id", &self.request_id())
            .field("meta", self.meta())
            .finish()
    }
}

/// Why a stream ended before `message_stop`. `Debug` withholds the parser's error of
/// [`InvalidEvent`](Self::InvalidEvent), which may quote the data.
#[non_exhaustive]
pub enum IncompleteCause {
    /// The API sent an `error` event (an `overloaded_error`, say) after the 200 response.
    ErrorEvent(ProviderError),
    /// The body ended cleanly at an event boundary, but no `message_stop` had arrived.
    EndOfStream,
    /// Reading the body failed (a connection lost mid-stream): Concord's `Transport` error,
    /// naming the endpoint, URL removed.
    Transport(concord::Error),
    /// The body is not a well-formed event stream: invalid UTF-8, or it ends inside a line or an
    /// event (Concord's SSE reader, `Format`).
    MalformedStream(ReadError),
    /// One event was larger than [`MAX_EVENT_BYTES`](crate::MAX_EVENT_BYTES) (Concord's SSE
    /// reader, `Limit`); it was not truncated.
    EventTooLarge(ReadError),
    /// An event's `data` is not JSON, or not the documented shape of its type. The serde error
    /// may quote the data: reach it with [`Incomplete::invalid_event_source`].
    InvalidEvent {
        /// The SSE event name.
        event: String,
        /// The parser's error (may quote the data).
        source: serde_json::Error,
    },
    /// The events break the documented flow (a delta before its block started, an SSE event name
    /// that differs from the data's `type`, `message_stop` with a block still open, ...).
    Protocol(&'static str),
    /// `next()` was called again after the stream had already failed; the first failure was
    /// returned then.
    AlreadyFailed,
}

impl IncompleteCause {
    /// A static description, without content.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ErrorEvent(_) => "the API sent an error event",
            Self::EndOfStream => "the body ended before message_stop",
            Self::Transport(_) => "reading the stream failed",
            Self::MalformedStream(_) => "the body is not a well-formed event stream",
            Self::EventTooLarge(_) => "an event exceeded the size limit",
            Self::InvalidEvent { .. } => "an event's data is not of the documented shape",
            Self::Protocol(_) => "the events break the documented order",
            Self::AlreadyFailed => "the stream had already failed",
        }
    }
}

impl fmt::Debug for IncompleteCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ErrorEvent(error) => f.debug_tuple("ErrorEvent").field(error).finish(),
            Self::EndOfStream => f.write_str("EndOfStream"),
            Self::Transport(error) => f.debug_tuple("Transport").field(error).finish(),
            Self::MalformedStream(read) => f.debug_tuple("MalformedStream").field(read).finish(),
            Self::EventTooLarge(read) => f.debug_tuple("EventTooLarge").field(read).finish(),
            Self::InvalidEvent { event, .. } => f
                .debug_struct("InvalidEvent")
                .field("event", event)
                .field("source", &format_args!("<withheld>"))
                .finish(),
            Self::Protocol(rule) => f.debug_tuple("Protocol").field(rule).finish(),
            Self::AlreadyFailed => f.write_str("AlreadyFailed"),
        }
    }
}

/// A stream that did not reach `message_stop`. It holds what arrived before
/// ([`partial`](Self::partial)), which is **not** a final message, and nothing was re-sent: a
/// continuation (the streaming page's "Error recovery") is a new request the application builds.
pub struct Incomplete {
    cause: IncompleteCause,
    partial: PartialMessage,
    events: u64,
    request_id: Option<String>,
}

impl Incomplete {
    pub(crate) fn new(
        cause: IncompleteCause,
        partial: PartialMessage,
        events: u64,
        request_id: Option<String>,
    ) -> Self {
        Self {
            cause,
            partial,
            events,
            request_id,
        }
    }

    /// Why the stream ended early.
    pub fn cause(&self) -> &IncompleteCause {
        &self.cause
    }

    /// The cause, taken (a [`IncompleteCause::Transport`] error by value, say).
    pub fn into_cause(self) -> IncompleteCause {
        self.cause
    }

    /// What had been assembled when the stream ended.
    pub fn partial(&self) -> &PartialMessage {
        &self.partial
    }

    /// Events received and parsed before the failure (the failing one excluded).
    pub fn events_received(&self) -> u64 {
        self.events
    }

    /// The `request-id` of the stream's response.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// The parser's error of [`IncompleteCause::InvalidEvent`], **unredacted** (it may quote the
    /// event data).
    pub fn invalid_event_source(&self) -> Option<&serde_json::Error> {
        match &self.cause {
            IncompleteCause::InvalidEvent { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl fmt::Display for Incomplete {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the message stream is incomplete (no message_stop): {}",
            self.cause.label()
        )?;
        match &self.cause {
            IncompleteCause::ErrorEvent(error) => write!(f, " ({})", error.error_type)?,
            IncompleteCause::Protocol(rule) => write!(f, " ({rule})")?,
            IncompleteCause::InvalidEvent { event, .. } => write!(f, " (event {event:?})")?,
            IncompleteCause::MalformedStream(read) | IncompleteCause::EventTooLarge(read) => {
                write!(f, " ({read})")?;
            }
            _ => {}
        }
        write!(f, ", after {} events", self.events)?;
        if let Some(id) = &self.request_id {
            write!(f, ", request {id}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Incomplete {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Incomplete")
            .field("cause", &self.cause)
            .field("partial", &self.partial)
            .field("events", &self.events)
            .field("request_id", &self.request_id)
            .finish()
    }
}
