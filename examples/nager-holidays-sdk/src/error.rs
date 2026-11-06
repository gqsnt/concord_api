//! Typed HTTP failure classification with Concord's original error and unread response.

use std::fmt;

use concord::ErrorKind as ConcordErrorKind;
use concord::reqwest::StatusCode;

/// An owned response head (status, version, all headers, operation) that survives a subsequent
/// body or decoding failure: Concord's own type. `Debug` prints header names only.
pub use concord::ResponseMeta;

/// SDK failure classification. No error-document schema is assumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// HTTP 400, classified solely by status; the body remains available and unread.
    InvalidRequest,
    /// Another non-2xx HTTP status; inspect [`Error::status`] for the actual code.
    Http,
    /// Concord's original build, transport, response metadata or decoding stage.
    Concord(ConcordErrorKind),
}

/// A failed fetch, keeping the original Concord error and every received response head.
///
/// `Display`/`Debug` omit URLs, header values and response bodies. Use the explicit accessors
/// to inspect metadata or consume a non-2xx response; no fabricated status is substituted.
#[derive(Debug)]
pub struct Error {
    source: concord::Error,
    meta: Option<Box<ResponseMeta>>,
}

impl Error {
    /// Classifies HTTP 400 separately from other statuses and Concord failures.
    pub fn kind(&self) -> ErrorKind {
        match self.source.kind() {
            ConcordErrorKind::Status if self.source.status() == Some(StatusCode::BAD_REQUEST) => {
                ErrorKind::InvalidRequest
            }
            ConcordErrorKind::Status => ErrorKind::Http,
            kind => ErrorKind::Concord(kind),
        }
    }

    /// Actual response status, including on body failures; `None` if no head arrived.
    pub fn status(&self) -> Option<StatusCode> {
        self.meta.as_ref().map(|meta| meta.status())
    }

    /// Captured response head; absent for request-building or pre-head transport failures.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        self.meta.as_deref()
    }

    /// Original error, including operation identity, codec diagnostics and unread responses
    /// on status or media-type failures.
    pub fn concord(&self) -> &concord::Error {
        &self.source
    }

    /// Takes the original error. Its `into_response()` can recover an unread HTTP error
    /// body or refused media type. Bounding and interpreting that body belongs to the caller.
    pub fn into_concord(self) -> concord::Error {
        self.source
    }
}

impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta().map(Box::new);
        Self { source, meta }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.source, f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
