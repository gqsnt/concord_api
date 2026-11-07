//! Errors of a call, and the client's send policy that produces them.

use std::fmt;

use concord::reqwest::StatusCode;
use concord::{Endpoint, Json, Operation, Received, SendWith};

use crate::model::ErrorDocument;
use crate::streaming::DiscoveryError;

/// The most bytes read from a non-2xx response before giving up on its error document.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// A non-2xx answer: the endpoint, the status and, when the body was one, Mastodon's error
/// document (`{"error": "..."}`).
///
/// The status is authoritative: a body that is absent, larger than [`ERROR_BODY_LIMIT`], not
/// labelled JSON, not JSON or not that document leaves [`document`](Self::document) empty.
/// `Display` shows the endpoint and the status only — not the document's text (read it with
/// [`message`](Self::message)), a header value or the URL.
#[derive(Clone)]
pub struct ApiError {
    operation: Option<Operation>,
    status: StatusCode,
    document: Option<ErrorDocument>,
}

impl ApiError {
    /// Reads a non-2xx response: its body, up to [`ERROR_BODY_LIMIT`], as an error document
    /// labelled JSON.
    async fn read<T>(received: Received<T>) -> Self {
        let operation = received.operation();
        let status = received.status();
        let (_, document) = received
            .document::<Json, ErrorDocument>(ERROR_BODY_LIMIT)
            .await;
        Self {
            operation,
            status,
            document: document.ok(),
        }
    }

    /// The declared endpoint that was answered (`Api.public_timeline`, ...).
    pub fn operation(&self) -> Option<Operation> {
        self.operation
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Mastodon's error document, when the body was one.
    pub fn document(&self) -> Option<&ErrorDocument> {
        self.document.as_ref()
    }

    /// The document's `error` text (`"Record not found"`), when there was a document.
    pub fn message(&self) -> Option<&str> {
        self.document.as_ref().map(ErrorDocument::error)
    }

    /// `401`: the token is missing or invalid, and the instance requires one for this call (the
    /// public timeline when it disabled unauthenticated access, the streaming API always).
    pub fn is_unauthorized(&self) -> bool {
        self.status == StatusCode::UNAUTHORIZED
    }

    /// `404`: no such status (or it is not visible to this caller).
    pub fn is_not_found(&self) -> bool {
        self.status == StatusCode::NOT_FOUND
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.operation {
            write!(f, "{operation}: ")?;
        }
        write!(f, "Mastodon answered {}", self.status.as_u16())
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("operation", &self.operation)
            .field("status", &self.status.as_u16())
            .field("document", &self.document.is_some())
            .finish()
    }
}

impl std::error::Error for ApiError {}

/// Why a call failed: the error of every `send()` of this SDK and of
/// [`Mastodon::discover_streaming`](crate::Mastodon::discover_streaming).
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Concord's error, kept whole and redacted by Concord: the request was not built (`Build`),
    /// no answer arrived or the body broke off (`Transport`), a 2xx head was refused before its
    /// body was read (`Metadata`: a status that is not JSON, a stream that is not
    /// `text/event-stream`), a finite body did not decode (`Decode`) or was too long (`Limit`).
    Request(concord::Error),
    /// The server answered non-2xx.
    Api(Box<ApiError>),
    /// The streaming host could not be determined, or belongs to another instance.
    Discovery(DiscoveryError),
}

impl Error {
    /// The non-2xx answer, if this is one.
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Self::Api(api) => Some(api),
            _ => None,
        }
    }

    /// Concord's error, if this is one.
    pub fn request(&self) -> Option<&concord::Error> {
        match self {
            Self::Request(error) => Some(error),
            _ => None,
        }
    }

    /// The discovery error, if this is one.
    pub fn discovery(&self) -> Option<&DiscoveryError> {
        match self {
            Self::Discovery(error) => Some(error),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Request(error)
    }
}

impl From<DiscoveryError> for Error {
    fn from(error: DiscoveryError) -> Self {
        Self::Discovery(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => error.fmt(f),
            Self::Api(error) => error.fmt(f),
            Self::Discovery(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// The send policy of the declaration (`send MastodonPolicy -> Result<_, Error>;`): one
/// execution with `Prepared::receive`; a non-2xx answer becomes [`Error::Api`] with its error
/// document read (bounded); anything else is handled as declared — a finite document is decoded,
/// the stream endpoint returns its open [`PublicStream`](crate::PublicStream) without reading the
/// body. No retry, no redirect handling of its own.
#[derive(Clone, Copy, Debug, Default)]
pub struct MastodonPolicy;

impl<T: Send> SendWith<T> for MastodonPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            return Err(Error::Api(Box::new(ApiError::read(received).await)));
        }
        Ok(received.decode().await?)
    }
}
