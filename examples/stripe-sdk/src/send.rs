//! Sending an operation and keeping Stripe's response metadata and error documents.
//!
//! Every generated operation keeps its ordinary `send()` (Concord's typed value on success, nothing else).
//! [`send`] is the SDK's alternative for callers that need what Stripe puts around the value:
//! the `Request-Id`, `Idempotent-Replayed`, `Stripe-Should-Retry` and
//! `Stripe-Rate-Limited-Reason` headers, and for a non-2xx status the typed error document
//! (`{"error": {"type": .., "code": .., ..}}`).
//!
//! It uses Concord's response boundary ([`concord::Prepared::receive`]): exactly one execution,
//! which returns the response for any status with its body unread. A transport failure is
//! Concord's `Transport` error (URL removed) without metadata: no response arrived. Otherwise the
//! head is projected into [`ResponseMeta`]; a non-2xx status reads Stripe's error document with
//! [`concord::Received::document`], bounded by [`MAX_ERROR_BODY`] and ignoring the media type.
//! A 2xx goes to the endpoint's own handling ([`concord::Received::decode_with_meta`]: the `Json`
//! head check, then decoding), and a failure there keeps the metadata. Nothing is retried.
//!
//! # Diagnostics
//!
//! [`Error`]'s `Display` and `Debug` show the endpoint, the status, Stripe's error `type` and
//! `code`, the request id and the rate-limit reason. They never show the secret key, the
//! `Stripe-Account` value, an idempotency key, a cursor, an error `message` or `param` (which
//! can quote submitted values) or a response body. Those are reachable through named accessors
//! ([`ApiError::message`], [`ApiFailure::error`]).

use std::fmt;

use concord::bytes::Bytes;
use concord::reqwest::{self, StatusCode};
use concord::{
    Decode, ErrorKind, IgnoreContentType, Json, Operation, Prepared, Received, ResponseHead,
};
use serde::Deserialize;

/// The largest error document [`send`] reads (Stripe's are a few hundred bytes).
pub const MAX_ERROR_BODY: usize = 64 * 1024;

/// Why Stripe rate-limited a request (`Stripe-Rate-Limited-Reason`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RateLimitedReason {
    /// `global-rate`.
    GlobalRate,
    /// `endpoint-rate`.
    EndpointRate,
    /// `global-concurrency`.
    GlobalConcurrency,
    /// `endpoint-concurrency`.
    EndpointConcurrency,
    /// `resource-specific`.
    ResourceSpecific,
    /// Another value, as sent.
    Other(String),
}

impl RateLimitedReason {
    fn parse(value: &str) -> Self {
        match value {
            "global-rate" => Self::GlobalRate,
            "endpoint-rate" => Self::EndpointRate,
            "global-concurrency" => Self::GlobalConcurrency,
            "endpoint-concurrency" => Self::EndpointConcurrency,
            "resource-specific" => Self::ResourceSpecific,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The header value.
    pub fn as_str(&self) -> &str {
        match self {
            Self::GlobalRate => "global-rate",
            Self::EndpointRate => "endpoint-rate",
            Self::GlobalConcurrency => "global-concurrency",
            Self::EndpointConcurrency => "endpoint-concurrency",
            Self::ResourceSpecific => "resource-specific",
            Self::Other(value) => value,
        }
    }
}

/// What Stripe said about a response, besides its body. Captured from the response head before
/// the body is read; every field is `None`/`false` when the header was absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseMeta {
    /// The HTTP status.
    pub status: u16,
    /// `Request-Id` (`req_...`), Stripe's identifier of this request.
    pub request_id: Option<String>,
    /// `Idempotent-Replayed: true`: Stripe returned the saved result of an earlier request with
    /// the same idempotency key instead of executing again.
    pub idempotent_replayed: bool,
    /// `Stripe-Should-Retry`: Stripe's own retry advice, when it gives one.
    pub should_retry: Option<bool>,
    /// `Stripe-Rate-Limited-Reason`, on a rate-limited `429`. A `429` without it is not a rate
    /// limit (Stripe documents lock timeouts as the other cause).
    pub rate_limited_reason: Option<RateLimitedReason>,
    /// `Stripe-Version` of the response, if Stripe sent one.
    pub stripe_version: Option<String>,
}

impl ResponseMeta {
    /// Copies the Stripe headers of a response whose body is not read.
    pub fn capture(response: &reqwest::Response) -> Self {
        Self::from_head(&ResponseHead::of(response))
    }

    /// Copies the Stripe headers of a response head (a [`concord::Received::head`]).
    pub fn from_head(head: &ResponseHead<'_>) -> Self {
        let headers = head.headers();
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        Self {
            status: head.status().as_u16(),
            request_id: text("request-id"),
            idempotent_replayed: text("idempotent-replayed").is_some_and(|v| v == "true"),
            should_retry: text("stripe-should-retry").and_then(|v| match v.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }),
            rate_limited_reason: text("stripe-rate-limited-reason")
                .map(|v| RateLimitedReason::parse(&v)),
            stripe_version: text("stripe-version"),
        }
    }

    /// Whether this response is a documented rate limit (`429` with a reason header).
    pub fn is_rate_limited(&self) -> bool {
        self.status == 429 && self.rate_limited_reason.is_some()
    }
}

/// A decoded value with its response metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Response<T> {
    /// The endpoint's typed output.
    pub value: T,
    /// The response metadata.
    pub meta: ResponseMeta,
}

/// Stripe's error `type`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorType {
    /// `api_error`.
    ApiError,
    /// `card_error`.
    CardError,
    /// `idempotency_error`: the key was reused with a different endpoint or parameters.
    IdempotencyError,
    /// `invalid_request_error`.
    InvalidRequestError,
    /// A value this SDK does not know.
    #[serde(other)]
    Other,
}

impl ApiErrorType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ApiError => "api_error",
            Self::CardError => "card_error",
            Self::IdempotencyError => "idempotency_error",
            Self::InvalidRequestError => "invalid_request_error",
            Self::Other => "other",
        }
    }
}

/// Stripe's error object (the `error` member of an error response).
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
pub struct ApiError {
    /// The error type.
    #[serde(rename = "type")]
    pub kind: ApiErrorType,
    /// The error code, such as `resource_missing`, `rate_limit` or `idempotency_key_in_use`.
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    #[sensitive]
    message: Option<String>,
    #[serde(default)]
    #[sensitive]
    param: Option<String>,
    /// Link to the code's documentation.
    #[serde(default)]
    #[sensitive]
    pub doc_url: Option<String>,
}

impl ApiError {
    /// The human-readable message, **unredacted**: Stripe messages can quote submitted values.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The parameter the error concerns, **unredacted**.
    pub fn param(&self) -> Option<&str> {
        self.param.as_deref()
    }
}

#[derive(Deserialize)]
struct ErrorDocument {
    error: ApiError,
}

/// Why an error document could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentProblem {
    /// The body was larger than [`MAX_ERROR_BODY`].
    TooLarge,
    /// Reading the body failed.
    Read,
    /// The body is not a Stripe error document.
    NotADocument,
}

/// A non-2xx response from Stripe.
pub struct ApiFailure {
    endpoint: Option<&'static str>,
    meta: ResponseMeta,
    error: Result<ApiError, DocumentProblem>,
}

impl ApiFailure {
    /// The endpoint, such as `Stripe.customers.create`.
    pub fn endpoint(&self) -> Option<&'static str> {
        self.endpoint
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        StatusCode::from_u16(self.meta.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// The response metadata (request id, rate-limit reason, retry advice).
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// Stripe's error object, or why it could not be read.
    pub fn error(&self) -> Result<&ApiError, DocumentProblem> {
        self.error.as_ref().map_err(|p| *p)
    }
}

impl fmt::Debug for ApiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiFailure")
            .field("endpoint", &self.endpoint)
            .field("meta", &self.meta)
            .field("error", &self.error)
            .finish()
    }
}

/// A cursor-traversal protocol violation detected by [`crate::pages`]. `Display` never shows a
/// cursor or an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProtocolError {
    /// `has_more: true` with an empty `data`: no object can name the next page.
    EmptyNonTerminalPage,
    /// The next cursor equals the cursor that produced this page: following it would repeat.
    CursorDidNotAdvance,
    /// A resume cursor of the other direction (`ending_before` for a `starting_after` chain or
    /// the reverse): the two are mutually exclusive.
    IncompatibleDirection,
    /// The request template already sets `starting_after` or `ending_before`; the traversal
    /// owns those parameters.
    TemplateHasCursor,
    /// A resume cursor produced by another list (another account, filter, page size or
    /// expansion set).
    CursorBindingMismatch,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyNonTerminalPage => {
                "list page has `has_more: true` but no object to continue from"
            }
            Self::CursorDidNotAdvance => "list page repeats the cursor that requested it",
            Self::IncompatibleDirection => {
                "cursor direction is incompatible with the traversal (starting_after and ending_before are exclusive)"
            }
            Self::TemplateHasCursor => {
                "list template already sets starting_after or ending_before"
            }
            Self::CursorBindingMismatch => "cursor belongs to another list request",
        })
    }
}

/// An SDK error. See the [module documentation](self) for what its output shows.
#[non_exhaustive]
pub enum Error {
    /// Stripe answered with a non-2xx status.
    Api(Box<ApiFailure>),
    /// A Concord error: building the request, the transport, a refused response head, or a 2xx
    /// body that does not decode. `meta` is `Some` once a response head was received.
    Concord {
        /// The Concord error, unchanged.
        error: concord::Error,
        /// The response metadata, if a head arrived.
        meta: Option<ResponseMeta>,
    },
    /// A list page that breaks the cursor protocol.
    Protocol {
        /// What was wrong.
        problem: ProtocolError,
        /// The response metadata of the offending page, if one was received.
        meta: Option<ResponseMeta>,
    },
}

impl Error {
    /// The failure's response metadata, if a response head was received.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Api(failure) => Some(&failure.meta),
            Self::Concord { meta, .. } | Self::Protocol { meta, .. } => meta.as_ref(),
        }
    }

    /// The API failure, for a non-2xx status.
    pub fn api(&self) -> Option<&ApiFailure> {
        match self {
            Self::Api(failure) => Some(failure),
            _ => None,
        }
    }

    /// The Concord error, if this is one.
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Self::Concord { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The protocol problem, if this is one.
    pub fn protocol(&self) -> Option<ProtocolError> {
        match self {
            Self::Protocol { problem, .. } => Some(*problem),
            _ => None,
        }
    }

    /// Whether no response was received: the request may or may not have reached Stripe (a
    /// connection failure or a lost response). For a write, only the application can decide
    /// whether to repeat it, with the same idempotency key.
    pub fn is_response_lost(&self) -> bool {
        matches!(self, Self::Concord { error, meta: None } if error.kind() == ErrorKind::Transport)
    }

    /// Whether Stripe refused the request as a reused idempotency key with different
    /// parameters (`type: idempotency_error`).
    pub fn is_idempotency_error(&self) -> bool {
        self.api()
            .and_then(|f| f.error().ok())
            .is_some_and(|e| e.kind == ApiErrorType::IdempotencyError)
    }

    /// Whether Stripe rate-limited the request (`429` with `Stripe-Rate-Limited-Reason`).
    pub fn is_rate_limited(&self) -> bool {
        self.meta().is_some_and(ResponseMeta::is_rate_limited)
    }

    pub(crate) fn protocol_error(problem: ProtocolError, meta: Option<ResponseMeta>) -> Self {
        Self::Protocol { problem, meta }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        let meta = error.head().map(|head| ResponseMeta::from_head(&head));
        Self::Concord { error, meta }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(failure) => {
                write!(
                    f,
                    "{}: Stripe returned HTTP {}",
                    failure.endpoint.unwrap_or("Stripe request"),
                    failure.meta.status
                )?;
                match &failure.error {
                    Ok(error) => {
                        write!(f, " ({}", error.kind.as_str())?;
                        if let Some(code) = &error.code {
                            write!(f, "/{code}")?;
                        }
                        f.write_str(")")?;
                    }
                    Err(problem) => write!(f, " (error document unreadable: {problem:?})")?,
                }
                if let Some(reason) = &failure.meta.rate_limited_reason {
                    write!(f, ", rate limited: {}", reason.as_str())?;
                }
                if let Some(id) = &failure.meta.request_id {
                    write!(f, ", request {id}")?;
                }
                Ok(())
            }
            Self::Concord { error, .. } => fmt::Display::fmt(error, f),
            Self::Protocol { problem, meta } => {
                write!(f, "Stripe list protocol error: {problem}")?;
                if let Some(id) = meta.as_ref().and_then(|m| m.request_id.as_ref()) {
                    write!(f, " (request {id})")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(failure) => f.debug_tuple("Api").field(failure).finish(),
            Self::Concord { error, meta } => f
                .debug_struct("Concord")
                .field("error", error)
                .field("meta", meta)
                .finish(),
            Self::Protocol { problem, meta } => f
                .debug_struct("Protocol")
                .field("problem", problem)
                .field("meta", meta)
                .finish(),
        }
    }
}

impl std::error::Error for Error {}

/// Sends a prepared operation once and returns its value with Stripe's response metadata, or an
/// [`Error`] that keeps the metadata and the typed error document. See the
/// [module documentation](self).
///
/// # Errors
///
/// [`Error::Api`] for a non-2xx status, [`Error::Concord`] for transport, response-head and
/// decoding failures.
pub async fn send<T>(prepared: Prepared<T>) -> Result<Response<T>, Error> {
    // A transport failure: `Error::Concord` with `meta: None` (no response arrived).
    let received = prepared.receive().await?;
    if !received.status().is_success() {
        let endpoint = received
            .operation()
            .filter(Operation::is_endpoint)
            .map(|operation| operation.name());
        return Err(api_failure(endpoint, received).await);
    }
    let (value, meta) = received.decode_with_meta().await?;
    Ok(Response {
        value,
        meta: ResponseMeta::from_head(&meta.head()),
    })
}

/// A non-2xx answer: Stripe's error document, read explicitly from the unread body.
async fn api_failure<T>(endpoint: Option<&'static str>, received: Received<T>) -> Error {
    // Stripe error documents are interpreted even when a proxy labels them as text.
    let (meta, document) = received
        .document::<IgnoreContentType<Json>, ErrorDocument>(MAX_ERROR_BODY)
        .await;
    let error = document
        .map(|document| document.error)
        .map_err(|error| match error.kind() {
            ErrorKind::Decode => DocumentProblem::NotADocument,
            ErrorKind::Limit => DocumentProblem::TooLarge,
            _ => DocumentProblem::Read,
        });
    Error::Api(Box::new(ApiFailure {
        endpoint,
        meta: ResponseMeta::from_head(&meta.head()),
        error,
    }))
}

/// Decodes a Stripe error document (`{"error": {..}}`) from bytes, with no head check.
///
/// # Errors
///
/// [`DocumentProblem::NotADocument`] when the bytes are not one.
pub fn parse_document(body: Bytes) -> Result<ApiError, DocumentProblem> {
    <Json as Decode<ErrorDocument>>::decode_body((), body)
        .map(|document| document.error)
        .map_err(|_| DocumentProblem::NotADocument)
}
