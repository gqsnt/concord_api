use concord::{ResponseMeta, bytes::Bytes, reqwest::StatusCode};
use std::{
    fmt,
    time::{Duration, SystemTime},
};

/// Provider HTTP failure. Status remains authoritative even when its body is unreadable.
pub struct HttpError {
    /// Status, header fields and operation; raw header access is unredacted.
    pub meta: ResponseMeta,
    /// At most 16 KiB of raw diagnostic bytes, or the read/limit failure.
    /// No error schema is assumed for public, XML, JSON or intermediary error pages.
    pub body: Result<Bytes, concord::Error>,
}

impl HttpError {
    /// Shared strict Retry-After parsing with an explicit wall clock; no automatic retry.
    pub fn retry_after(
        &self,
        now: SystemTime,
    ) -> Result<Option<Duration>, concord_retry::http::RetryAfterError> {
        concord_retry::http::retry_after(
            self.meta
                .headers()
                .get_all("retry-after")
                .iter()
                .map(|v| v.as_bytes()),
            now,
        )
    }
}

impl fmt::Debug for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpError")
            .field("meta", &self.meta)
            .field("body_bytes", &self.body.as_ref().ok().map(Bytes::len))
            .field(
                "body_error",
                &self.body.as_ref().err().map(concord::Error::kind),
            )
            .finish()
    }
}

/// SDK failures. Formatting never includes queries, cursor marks, headers or response bodies.
#[derive(Debug)]
pub enum Error {
    /// An input was refused locally; the static reason contains no input value.
    InvalidInput(&'static str),
    /// Concord build, transport, metadata, decode or body-limit failure.
    Call(concord::Error),
    /// Non-2xx response, with a bounded diagnostic body.
    Http(Box<HttpError>),
    /// A successful response contradicted this SDK's envelope rules.
    InvalidPage(&'static str),
    /// A cursor returned to an already requested page, including cycles of several pages.
    RepeatedCursor,
    /// No publication matched the exact source and identifier.
    NotFound,
    /// A core lookup returned a different identity or several records.
    UnexpectedRecord,
}

impl Error {
    /// Status whenever a head was received, including a decode or mid-body failure.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Http(e) => Some(e.meta.status()),
            Self::Call(e) => e.head().map(|h| h.status()),
            _ => None,
        }
    }

    /// Underlying Concord failure for explicit stage or metadata inspection.
    pub fn call_error(&self) -> Option<&concord::Error> {
        match self {
            Self::Call(e) => Some(e),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Call(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(reason) => write!(f, "invalid Europe PMC input: {reason}"),
            Self::Call(e) => write!(f, "Europe PMC call failed: {e}"),
            Self::Http(e) => write!(f, "Europe PMC HTTP {}", e.meta.status()),
            Self::InvalidPage(reason) => write!(f, "invalid Europe PMC page: {reason}"),
            Self::RepeatedCursor => f.write_str("Europe PMC cursor repeated; traversal stopped"),
            Self::NotFound => f.write_str("Europe PMC citation was not found"),
            Self::UnexpectedRecord => {
                f.write_str("Europe PMC citation lookup returned an unexpected identity or count")
            }
        }
    }
}

impl std::error::Error for Error {}
