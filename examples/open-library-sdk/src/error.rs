use std::{error::Error as StdError, fmt};

use concord::{ResponseMeta, reqwest::StatusCode};
use serde_json::Value;

/// The kind of resource expected by an identifier input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordKind {
    /// A work (`OL` + digits + `W`).
    Work,
    /// An author (`OL` + digits + `A`).
    Author,
}

/// Failure of an ordinary SDK `send()` or identifier conversion.
///
/// Formatting omits request values, response header values and document contents. There is no
/// automatic source chain; underlying Concord details require deliberate accessor use.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Missing or invalid selected ID. No request was constructed by the conversion.
    InvalidId(RecordKind),
    /// Build, transport, media, JSON shape/syntax or body-limit failure.
    Concord(concord::Error),
    /// Any non-2xx status, even when its body cannot be decoded as JSON.
    Http(Box<HttpError>),
}

impl Error {
    /// HTTP status once a response head arrived, including unsuccessful success-body decoding.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::InvalidId(_) => None,
            Self::Concord(error) => error.head().map(|head| head.status()),
            Self::Http(error) => Some(error.meta.status()),
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
            Self::InvalidId(kind) => write!(f, "missing or invalid Open Library {kind:?} ID"),
            Self::Concord(error) => fmt::Display::fmt(error, f),
            Self::Http(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl StdError for Error {}

/// An HTTP failure with its original status and the bounded, untrusted JSON document outcome.
pub struct HttpError {
    pub(crate) meta: ResponseMeta,
    pub(crate) document: Result<Value, concord::Error>,
}

impl HttpError {
    /// The original response head. Header values are accessible explicitly; Debug lists names.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// Untrusted error JSON, or why it was unreadable. No provider error schema is assumed.
    ///
    /// This accessor may reveal secrets echoed by a proxy or provider. A nested Concord error's
    /// native response/body/source accessors have the same deliberate disclosure boundary.
    pub fn document_unredacted(&self) -> Result<&Value, &concord::Error> {
        self.document.as_ref()
    }
}

impl fmt::Debug for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpError")
            .field("meta", &self.meta)
            .field(
                "document_error_kind",
                &self.document.as_ref().err().map(|e| e.kind()),
            )
            .finish()
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Open Library HTTP {}", self.meta.status().as_u16())
    }
}

impl StdError for HttpError {}
