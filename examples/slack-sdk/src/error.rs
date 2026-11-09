//! Errors retain response metadata, including on malformed bodies and rate limits.

use std::{fmt, time::Duration};

use concord::{Operation, bytes::Bytes, reqwest::header::HeaderMap};

use crate::{ResponseMetadata, wire::Envelope};

/// HTTP metadata captured before consuming a body. Header values are accessible but not logged
/// by `Debug`. No response URL is retained because it can contain an opaque cursor.
#[derive(Clone)]
pub struct ResponseMeta {
    /// HTTP status, including 200 on an application-level failure.
    pub status: concord::reqwest::StatusCode,
    /// All response headers, including request IDs and raw `Retry-After` values.
    pub headers: HeaderMap,
    /// Identity supplied by the declared Concord operation.
    pub operation: Option<Operation>,
}

impl ResponseMeta {
    pub(crate) fn capture(head: concord::ResponseHead<'_>, operation: Option<Operation>) -> Self {
        Self {
            status: head.status(),
            headers: head.headers().clone(),
            operation,
        }
    }

    /// Slack's request ID, when present and representable as text.
    pub fn request_id(&self) -> Option<&str> {
        self.headers.get("x-slack-req-id")?.to_str().ok()
    }

    /// Interpret exactly one `Retry-After` as unsigned decimal seconds (Slack's documented
    /// representation). Missing, repeated, invalid, and overflowing values never imply zero.
    pub fn retry_after(&self) -> RetryAfter {
        let mut values = self.headers.get_all("retry-after").iter();
        let Some(value) = values.next() else {
            return RetryAfter::Missing;
        };
        if values.next().is_some() {
            return RetryAfter::Invalid;
        }
        let Ok(text) = value.to_str() else {
            return RetryAfter::Invalid;
        };
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return RetryAfter::Invalid;
        }
        match text.parse::<u64>() {
            Ok(seconds) => RetryAfter::Seconds(Duration::from_secs(seconds)),
            Err(_) => RetryAfter::Invalid,
        }
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status)
            .field("operation", &self.operation)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// A rate-limit delay; the SDK reports it without sleeping or retrying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAfter {
    /// One valid header, including the valid value zero.
    Seconds(Duration),
    /// No header arrived.
    Missing,
    /// Not one unsigned decimal integer of seconds.
    Invalid,
}

/// Useful Slack error codes for this slice, with a forward-compatible unknown case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    /// No accessible conversation with this ID.
    ChannelNotFound,
    /// Expired, malformed, or incorrectly encoded continuation.
    InvalidCursor,
    /// Token lacks the required permissions.
    MissingScope,
    /// Token was refused.
    InvalidAuth,
    /// Slack's body-level rate-limit code.
    RateLimited,
    /// A code not modeled by this slice, kept verbatim.
    Other(String),
}

impl ErrorCode {
    /// Exact Slack spelling.
    pub fn as_str(&self) -> &str {
        match self {
            Self::ChannelNotFound => "channel_not_found",
            Self::InvalidCursor => "invalid_cursor",
            Self::MissingScope => "missing_scope",
            Self::InvalidAuth => "invalid_auth",
            Self::RateLimited => "ratelimited",
            Self::Other(code) => code,
        }
    }
}

impl From<String> for ErrorCode {
    fn from(code: String) -> Self {
        match code.as_str() {
            "channel_not_found" => Self::ChannelNotFound,
            "invalid_cursor" => Self::InvalidCursor,
            "missing_scope" => Self::MissingScope,
            "invalid_auth" => Self::InvalidAuth,
            "ratelimited" => Self::RateLimited,
            _ => Self::Other(code),
        }
    }
}

/// A well-formed Slack `ok: false` body. Values are server data, available for explicit inspection.
#[derive(Debug)]
pub struct ProviderError {
    /// Machine-readable error code, including unknown future codes.
    pub code: ErrorCode,
    /// Required scopes, when supplied.
    pub needed: Option<String>,
    /// Granted scopes, when supplied.
    pub provided: Option<String>,
    /// Nonfatal warning accompanying the failure, if supplied.
    pub warning: Option<String>,
    /// Messages, warnings, and continuation metadata from the error body.
    pub response_metadata: Option<ResponseMetadata>,
}

/// A syntactically valid JSON body with an inconsistent Slack envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeProblem {
    /// `ok: false` omitted a nonempty `error` code.
    MissingError,
    /// `ok: true` included an `error` code.
    ConflictingError,
    /// `ok: true` omitted the users array or returned null.
    MissingMembers,
    /// `ok: true` omitted the conversation or returned null.
    MissingChannel,
}

/// A bounded non-success body. Raw bytes and read errors require explicit access; `Debug`
/// prints neither the body nor provider messages. A body failure never hides the HTTP status.
pub struct ErrorBody {
    pub(crate) read: Result<Bytes, concord::Error>,
    provider: Option<ProviderError>,
}

impl ErrorBody {
    pub(crate) fn from_failure(
        failure: concord::Failure<Envelope<serde_json::Map<String, serde_json::Value>>>,
    ) -> Self {
        let complete = !matches!(
            failure.state(),
            concord::Document::TooLong { .. } | concord::Document::Broken(_)
        );
        let bytes = failure.body().clone();
        let (_, document) = failure.into_document();
        let (read, provider) = match document {
            Ok(envelope) => {
                let provider = if envelope.ok {
                    None
                } else {
                    envelope.into_provider().ok()
                };
                (Ok(bytes), provider)
            }
            Err(error) if !complete => (Err(error), None),
            Err(_) => (Ok(bytes), None),
        };
        Self { read, provider }
    }

    /// Complete body bytes only when reading succeeded within the SDK's 64 KiB limit.
    pub fn bytes(&self) -> Option<&[u8]> {
        self.read.as_deref().ok()
    }

    /// Concord's operation-labeled limit or transport error, when reading failed.
    pub fn read_error(&self) -> Option<&concord::Error> {
        self.read.as_ref().err()
    }

    /// Parsed application failure, if the HTTP error body was a valid Slack failure envelope.
    pub fn provider_error(&self) -> Option<&ProviderError> {
        self.provider.as_ref()
    }
}

impl fmt::Debug for ErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorBody")
            .field("bytes", &self.bytes().map(<[u8]>::len))
            .field("read_error", &self.read_error())
            .finish_non_exhaustive()
    }
}

/// SDK errors. [`Self::meta`] preserves headers whenever a response head arrived.
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Request construction, transport, content type, JSON, or body-read failure.
    Concord {
        /// Original Concord error, retaining its declared operation identity.
        error: concord::Error,
        /// Response head, if execution reached it.
        meta: Option<Box<ResponseMeta>>,
    },
    /// HTTP 200 (or another 2xx) with `ok: false`.
    Api {
        /// Typed provider failure.
        error: Box<ProviderError>,
        /// HTTP response metadata.
        meta: Box<ResponseMeta>,
    },
    /// HTTP 429, regardless of whether its body is readable JSON.
    RateLimited {
        /// Parsed delay without any automatic action.
        retry_after: RetryAfter,
        /// HTTP response metadata, including the raw header.
        meta: Box<ResponseMeta>,
        /// Bounded body or read failure, without masking rate limiting.
        body: Box<ErrorBody>,
    },
    /// Another non-2xx HTTP response, including unexpected redirects.
    Http {
        /// HTTP response metadata.
        meta: Box<ResponseMeta>,
        /// Bounded body or read failure.
        body: Box<ErrorBody>,
    },
    /// Parsed JSON contradicted the declared success/failure shape.
    Envelope {
        /// Structural problem, never treated as an empty successful page.
        problem: EnvelopeProblem,
        /// HTTP response metadata.
        meta: Box<ResponseMeta>,
    },
}

impl Error {
    /// Response metadata on any failure after headers arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Concord { meta, .. } => meta.as_deref(),
            Self::Api { meta, .. }
            | Self::RateLimited { meta, .. }
            | Self::Http { meta, .. }
            | Self::Envelope { meta, .. } => Some(meta),
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        let meta = error
            .head()
            .map(|head| Box::new(ResponseMeta::capture(head, error.operation())));
        Self::Concord { error, meta }
    }
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Concord { error, .. } => std::fmt::Display::fmt(error, f),
        Error::Api { .. } => f.write_str("Slack application failure"),
        Error::RateLimited { .. } => f.write_str("Slack rate limit (HTTP 429)"),
        Error::Http { meta, .. } => write!(f, "Slack HTTP failure ({})", meta.status),
        Error::Envelope { problem, .. } => write!(f, "invalid Slack envelope: {problem:?}"),
    }
}

impl<T> Envelope<T> {
    pub(crate) fn into_provider(self) -> Result<ProviderError, EnvelopeProblem> {
        let code = self
            .error
            .filter(|code| !code.is_empty())
            .ok_or(EnvelopeProblem::MissingError)?;
        Ok(ProviderError {
            code: code.into(),
            needed: self.needed,
            provided: self.provided,
            warning: self.warning,
            response_metadata: self.response_metadata,
        })
    }
}
