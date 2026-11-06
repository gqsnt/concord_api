use std::fmt;

use concord::reqwest::{StatusCode, Version, header::HeaderMap};
use concord::{CodecError, Operation, ResponseHead};
use serde::Deserialize;

/// Owned response metadata, captured before any body consumer.
///
/// `Debug` prints header names only; explicit header access is unredacted.
#[derive(Clone)]
pub struct ResponseMeta {
    /// Actual HTTP status; never synthesized from the JSON document.
    pub status: StatusCode,
    /// HTTP version observed by reqwest.
    pub version: Version,
    /// All response headers, including retry hints when supplied.
    pub headers: HeaderMap,
    /// Identity supplied by the generated endpoint, without request values.
    pub operation: Option<Operation>,
}

impl ResponseMeta {
    pub(crate) fn capture(head: &ResponseHead<'_>, operation: Option<Operation>) -> Self {
        Self {
            status: head.status(),
            version: head.version(),
            headers: head.headers().clone(),
            operation,
        }
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("operation", &self.operation)
            .finish()
    }
}

/// Documented provider error body; unknown fields are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Problem {
    /// Present in the website example, absent from the OpenAPI error schema.
    /// This is advisory; [`ResponseMeta::status`] remains authoritative.
    pub status: Option<u16>,
    /// Provider's error message. Untrusted response text; access deliberately.
    pub message: String,
}

/// Why a non-2xx response's optional error document could not be obtained.
#[derive(Debug)]
pub enum ErrorBodyError {
    /// Missing, repeated or non-JSON media type; body was not read.
    Metadata(CodecError),
    /// Body exceeded [`crate::ERROR_BODY_LIMIT`] or could not be read completely.
    Read(concord::Error),
    /// Body was JSON-labelled but did not decode as a [`Problem`].
    Decode(CodecError),
}

impl fmt::Display for ErrorBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(error) | Self::Decode(error) => fmt::Display::fmt(error, f),
            Self::Read(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ErrorBodyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Metadata(error) | Self::Decode(error) => Some(error),
            Self::Read(error) => Some(error),
        }
    }
}

/// Any non-2xx status, including malformed or missing provider error documents.
pub struct ProviderError {
    /// Response head, preserved regardless of error-body outcome.
    pub meta: ResponseMeta,
    /// Typed document or the precise failure while reading/decoding it.
    pub document: Result<Problem, ErrorBodyError>,
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("meta", &self.meta)
            .field("document_decoded", &self.document.is_ok())
            .field("document_error", &self.document.as_ref().err())
            .finish()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frankfurter HTTP {}", self.meta.status)
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.document.as_ref().err().map(|error| error as _)
    }
}

/// SDK failure. Response metadata survives both HTTP failures and body failures.
#[derive(Debug)]
pub enum Error {
    /// Non-2xx status, with a bounded typed error-document attempt.
    Provider(Box<ProviderError>),
    /// Request build, transport, success media-type or success decode failure.
    Concord {
        /// Original Concord error and its generated operation identity.
        source: concord::Error,
        /// Head captured before decoding; absent when no response arrived.
        meta: Option<Box<ResponseMeta>>,
    },
}

impl Error {
    /// Actual response metadata, if a head arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Provider(error) => Some(&error.meta),
            Self::Concord { meta, .. } => meta.as_deref(),
        }
    }

    /// Actual HTTP status, also retained after success-body read/decode failures.
    pub fn status(&self) -> Option<StatusCode> {
        self.meta().map(|meta| meta.status)
    }

    /// Generated operation identity, including for failures before a head arrived.
    pub fn operation(&self) -> Option<Operation> {
        match self {
            Self::Provider(error) => error.meta.operation,
            Self::Concord { source, .. } => source.operation(),
        }
    }
}

impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.response().map(|response| {
            Box::new(ResponseMeta::capture(
                &ResponseHead::of(response),
                source.operation(),
            ))
        });
        Self::Concord { source, meta }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provider(error) => fmt::Display::fmt(error, f),
            Self::Concord { source, .. } => fmt::Display::fmt(source, f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::Concord { source, .. } => Some(source),
        }
    }
}
