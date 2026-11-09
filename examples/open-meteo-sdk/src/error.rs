use std::fmt;

use concord::{CodecError, Operation, ResponseMeta, reqwest::StatusCode};
use serde::Deserialize;

/// Open-Meteo's documented JSON error envelope. HTTP status remains authoritative.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Problem {
    /// Provider error indicator.
    pub error: bool,
    /// Untrusted provider explanation, available through explicit field access.
    pub reason: String,
}

/// Failure to obtain the optional typed non-2xx document.
#[derive(Debug, concord::derive::Error)]
pub enum ErrorBodyError {
    /// The response was not labelled as JSON; body was not read.
    #[display("{0}")]
    #[from(skip)]
    Metadata(#[source] concord::Error),
    /// The body was too large or transport failed while reading it.
    #[display("{0}")]
    #[from(skip)]
    Read(#[source] concord::Error),
    /// JSON bytes did not match the documented error envelope.
    #[display("{0}")]
    #[from(skip)]
    Decode(#[source] concord::Error),
}
impl From<concord::Error> for ErrorBodyError {
    fn from(error: concord::Error) -> Self {
        match error.kind() {
            concord::ErrorKind::Metadata => Self::Metadata(error),
            concord::ErrorKind::Decode => Self::Decode(error),
            _ => Self::Read(error),
        }
    }
}

/// Any non-2xx response, even when its body is missing, malformed or too large.
pub struct ProviderError {
    /// Actual head, including status, headers, and generated operation identity.
    pub meta: ResponseMeta,
    /// Parsed provider document or the exact failure while obtaining it.
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
        write!(f, "Open-Meteo HTTP {}", self.meta.status())
    }
}
impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.document.as_ref().err().map(|error| error as _)
    }
}

/// Stage of success-response validation that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseErrorKind {
    /// Missing, repeated, malformed or mismatched Content-Type or charset.
    Media,
    /// Invalid JSON or invalid hourly array/selection structure.
    Json,
    /// Invalid CSV framing, tables, headers, row widths or selected numeric cells.
    Csv,
}

/// SDK failure. Response metadata survives all failures after the head arrived.
#[derive(Debug, concord::derive::Error)]
pub enum Error {
    /// Non-2xx response with a bounded typed error-document attempt.
    #[display("{0}")]
    #[from(concord::Error<crate::Problem>)]
    Provider(#[source] Box<ProviderError>),
    /// Request construction, transport, or bounded body-read failure.
    #[display("{source}")]
    Concord {
        #[source]
        /// Original Concord failure and its operation identity.
        source: concord::Error,
        /// Captured head, absent before a response arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// Success media or body validation failed; no partial forecast is returned.
    #[display("Open-Meteo {kind:?}: {source}")]
    Response {
        /// Validation stage.
        kind: ResponseErrorKind,
        #[source]
        /// Safe label, positions, and optional underlying diagnostic.
        source: CodecError,
        /// Actual response head.
        meta: Box<ResponseMeta>,
    },
}

impl Error {
    pub(crate) fn response(
        kind: ResponseErrorKind,
        source: CodecError,
        meta: &ResponseMeta,
    ) -> Self {
        Self::Response {
            kind,
            source,
            meta: Box::new(meta.clone()),
        }
    }
    /// Captured response head, when one arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Provider(e) => Some(&e.meta),
            Self::Concord { meta, .. } => meta.as_deref(),
            Self::Response { meta, .. } => Some(meta),
        }
    }
    /// Actual HTTP status, including after body parsing fails.
    pub fn status(&self) -> Option<StatusCode> {
        self.meta().map(ResponseMeta::status)
    }
    /// Declared operation identity, including transport failures before a response.
    pub fn operation(&self) -> Option<Operation> {
        match self {
            Self::Concord { source, .. } => source.operation(),
            _ => self.meta().and_then(ResponseMeta::operation),
        }
    }
}
impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta().map(Box::new);
        Self::Concord { source, meta }
    }
}

impl From<concord::Failure<crate::Problem>> for ProviderError {
    fn from(failure: concord::Failure<crate::Problem>) -> Self {
        let (meta, document) = failure.into_document();
        Self {
            meta,
            document: document.map_err(ErrorBodyError::from),
        }
    }
}
