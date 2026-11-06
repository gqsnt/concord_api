//! Errors of a call, and the client's send policy that produces them.

use std::fmt;
use std::time::Duration;

use concord::reqwest::StatusCode;
use concord::reqwest::header::RETRY_AFTER;
use concord::{Decode, Endpoint, Json, Operation, Received, SendWith};

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
    /// Reads a non-2xx response: `Retry-After` from the head, then the body up to
    /// [`ERROR_BODY_LIMIT`] decoded as ntfy's error document (its `Content-Type` is not judged).
    /// A body that is larger, unreadable, not JSON or not that document leaves
    /// [`document`](Self::document) empty; the status is always kept.
    async fn read<T>(received: Received<T>) -> Self {
        let operation = received.operation();
        let status = received.status();
        let retry_after = received
            .head()
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|text| text.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let document = match received.bytes_limited(ERROR_BODY_LIMIT).await {
            Ok(body) => <Json as Decode<ErrorDocument>>::decode_body((), body)
                .ok()
                .map(Box::new),
            Err(_) => None,
        };
        Self {
            operation,
            status,
            document,
            retry_after,
        }
    }

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

    /// `Retry-After` in seconds, when the server sent one (a `429`, typically). An HTTP-date form
    /// is not parsed.
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
pub enum NtfyError {
    /// Concord's error, kept whole and redacted by Concord: the request was not built (`Build`),
    /// no response arrived (`Transport`), a 2xx head was refused before its body was read
    /// (`Metadata`, the response kept unread: `concord::Error::into_response`), or a finite body
    /// did not decode (`Decode`).
    Request(concord::Error),
    /// The server answered non-2xx.
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

impl fmt::Display for NtfyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => error.fmt(f),
            Self::Api(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for NtfyError {}

/// The send policy of [`Ntfy`](crate::Ntfy) (`send NtfyPolicy;`): one execution with
/// `Prepared::receive`; a non-2xx answer becomes [`NtfyError::Api`] with its error document read
/// (bounded); anything else is decoded as declared — a stream endpoint returns its open
/// [`Events`](crate::Events) without reading the body. No retry.
#[derive(Clone, Copy, Debug, Default)]
pub struct NtfyPolicy;

impl<T: Send> SendWith<T> for NtfyPolicy {
    type Output = Result<T, NtfyError>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint
            .prepare()
            .map_err(NtfyError::Request)?
            .receive()
            .await
            .map_err(NtfyError::Request)?;
        if !received.status().is_success() {
            return Err(NtfyError::Api(ApiError::read(received).await));
        }
        received.decode().await.map_err(NtfyError::Request)
    }
}
