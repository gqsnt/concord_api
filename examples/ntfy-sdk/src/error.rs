//! Errors of a call, and the client's send policy that produces them.

use std::fmt;
use std::time::Duration;

use concord::reqwest::StatusCode;
use concord::reqwest::header::RETRY_AFTER;
use concord::{Endpoint, Operation, SendWith};

use crate::model::ErrorDocument;

/// The most bytes read from an error response before giving up on its document.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// A non-2xx answer: the status, ntfy's error document when the body was one, and `Retry-After`.
///
/// `Display` shows the endpoint, the status and ntfy's numeric `code` — not the document's `error`
/// text (read it with [`document`](Self::document)), a header value or the URL.
#[derive(Clone)]
pub struct ApiError {
    operation: Option<Operation>,
    status: StatusCode,
    /// Boxed, so that `NtfyError` stays small (clippy `result_large_err` in callers' code).
    document: Option<Box<ErrorDocument>>,
    retry_after: Option<Duration>,
}

impl ApiError {
    /// The declared endpoint that was answered.
    pub fn operation(&self) -> Option<Operation> {
        self.operation
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// ntfy's error document, when the body was one.
    pub fn document(&self) -> Option<&ErrorDocument> {
        self.document.as_deref()
    }

    /// ntfy's numeric error code (`40101` unauthorized, `40301` forbidden, `42901` too many
    /// requests, …), when the body was an error document that has one.
    pub fn code(&self) -> Option<u32> {
        self.document.as_ref().and_then(|d| d.code)
    }

    /// One unsigned decimal `Retry-After` field, with surrounding OWS allowed. Repeated,
    /// signed, overflowing and HTTP-date values are not parsed.
    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    /// `401`/`403`: the topic needs credentials, or the token is not allowed.
    pub fn is_auth(&self) -> bool {
        matches!(
            self.status,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        )
    }

    /// `429`: a rate, quota or bandwidth limit (see [`code`](Self::code) for which).
    pub fn is_rate_limited(&self) -> bool {
        self.status == StatusCode::TOO_MANY_REQUESTS
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.operation {
            write!(f, "{operation}: ")?;
        }
        write!(f, "ntfy answered {}", self.status.as_u16())?;
        match self.code() {
            Some(code) => write!(f, " (code {code})"),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("operation", &self.operation)
            .field("status", &self.status.as_u16())
            .field("code", &self.code())
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

impl std::error::Error for ApiError {}

/// Why a call failed: what every generated `send()` of [`Ntfy`](crate::Ntfy) returns as its error.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
pub enum NtfyError {
    /// Concord's error, kept whole and redacted by Concord: the request was not built (`Build`),
    /// no response arrived (`Transport`), a 2xx head was refused before its body was read
    /// (`Metadata`, the response kept unread: `concord::Error::into_response`), or a finite body
    /// did not decode (`Decode`).
    #[display("{0}")]
    Request(concord::Error),
    /// The server answered non-2xx.
    #[display("{0}")]
    #[from(concord::Error<crate::ErrorDocument>)]
    Api(ApiError),
}

impl NtfyError {
    /// The API error, for a non-2xx answer.
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Self::Api(api) => Some(api),
            Self::Request(_) => None,
        }
    }

    /// Concord's error, for every other failure.
    pub fn request(&self) -> Option<&concord::Error> {
        match self {
            Self::Request(error) => Some(error),
            Self::Api(_) => None,
        }
    }
}

/// The send policy of [`Ntfy`](crate::Ntfy) (`send NtfyPolicy -> Result<_, NtfyError>;`): one execution with
/// `Prepared::receive`; a non-2xx answer becomes [`NtfyError::Api`] with its error document read
/// (bounded); anything else is decoded as declared — a stream endpoint returns its open
/// [`Events`](crate::Events) without reading the body. No retry.
#[derive(Clone, Copy, Debug, Default)]
pub struct NtfyPolicy;

impl<T: Send> SendWith<T, crate::ErrorDocument> for NtfyPolicy {
    type Output = Result<T, NtfyError>;

    async fn send<E: Endpoint<T, crate::ErrorDocument> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint
            .prepare()
            .map_err(NtfyError::Request)?
            .error_body_limit(ERROR_BODY_LIMIT)
            .receive()
            .await
            .map_err(NtfyError::Request)?;

        received.decode().await.map_err(NtfyError::from)
    }
}

impl From<concord::Failure<ErrorDocument>> for ApiError {
    fn from(failure: concord::Failure<ErrorDocument>) -> Self {
        let operation = failure.operation();
        let status = failure.status();
        let retry_after = failure
            .meta()
            .head()
            .unsigned(RETRY_AFTER)
            .ok()
            .flatten()
            .map(Duration::from_secs);
        let (_, document) = failure.into_document();
        Self {
            operation,
            status,
            retry_after,
            document: document.ok().map(Box::new),
        }
    }
}
