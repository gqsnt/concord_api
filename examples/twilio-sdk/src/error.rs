//! Response metadata and bounded provider error decoding.

use std::fmt;

use concord::reqwest::{StatusCode, Version, header::HeaderMap};
use concord::{Decode, ErrorKind, Json, Operation, Prepared, Received};
use serde::Deserialize;

/// Maximum retained bytes of a non-success response body (64 KiB).
pub const MAX_ERROR_BODY: usize = 64 * 1024;

/// Metadata captured before consuming any response body, including failed decodes.
#[derive(Clone)]
pub struct ResponseMeta {
    /// Actual HTTP status, independent of any status claimed in an error document.
    pub status: StatusCode,
    /// Received HTTP version.
    pub version: Version,
    /// Identity automatically supplied by Concord's declaration.
    pub operation: Option<Operation>,
    headers: HeaderMap,
}

impl ResponseMeta {
    fn capture<T>(received: &Received<T>) -> Self {
        Self {
            status: received.status(),
            version: received.version(),
            operation: received.operation(),
            headers: received.head().headers().clone(),
        }
    }

    /// All response headers, unredacted (including duplicate values).
    ///
    /// Useful for provider request identifiers and retry advice when present. This SDK does
    /// not require any particular request-id header or invent one if absent.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("operation", &self.operation)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// One complete resource and the response metadata that accompanied it.
#[derive(Clone)]
pub struct Response<T> {
    /// The decoded resource; its fields can contain personal data.
    pub value: T,
    /// Metadata of the successful HTTP response.
    pub meta: ResponseMeta,
}

impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// Twilio's JSON exception document on an unsuccessful HTTP request.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderError {
    /// Status claimed by the body; [`ResponseMeta::status`] remains authoritative.
    pub status: u16,
    /// Twilio-specific code, when present.
    pub code: Option<i64>,
    /// Unredacted provider explanation, which can quote submitted data.
    pub message: String,
    /// Optional documentation link, retained as data and never fetched by this SDK.
    pub more_info: Option<String>,
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("status", &self.status)
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}

/// Why an HTTP failure did not yield a typed provider document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorDocumentProblem {
    /// Missing, repeated or incompatible JSON content type.
    ContentType,
    /// The error document exceeded [`MAX_ERROR_BODY`]; no truncated document is returned.
    TooLarge,
    /// The body stream failed before EOF (including a timeout).
    Read,
    /// Invalid JSON or a document missing required fields / containing wrong field types.
    Decode,
}

/// A request failure. An HTTP 200 message with `status: failed` is instead successful data.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A non-2xx response, with a provider exception or the reason it could not be decoded.
    Provider {
        /// Metadata retained even for malformed, oversized or truncated error documents.
        meta: Box<ResponseMeta>,
        /// Typed document, or a bounded failure classification (raw body is not retained).
        document: Result<ProviderError, ErrorDocumentProblem>,
    },
    /// A request build, transport, success content-type or success body decoding error.
    Concord {
        /// Original Concord error, preserving its kind, operation and available response.
        error: concord::Error,
        /// Present once the response head arrived, including failures while reading its body.
        meta: Option<Box<ResponseMeta>>,
    },
}

impl Error {
    /// Response metadata, or `None` if the request failed before receiving a head.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Provider { meta, .. } => Some(meta),
            Self::Concord { meta, .. } => meta.as_deref(),
        }
    }

    /// The provider document when a non-2xx body decoded successfully.
    pub fn provider_error(&self) -> Option<&ProviderError> {
        match self {
            Self::Provider {
                document: Ok(document),
                ..
            } => Some(document),
            _ => None,
        }
    }

    /// The original Concord error, if present.
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Self::Concord { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Operation identity, available even on a transport error without response metadata.
    pub fn operation(&self) -> Option<Operation> {
        self.meta()
            .and_then(|meta| meta.operation)
            .or_else(|| self.concord().and_then(concord::Error::operation))
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord { error, meta: None }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord { error, .. } => fmt::Display::fmt(error, f),
            Self::Provider { meta, document } => {
                if let Some(operation) = meta.operation {
                    write!(f, "{operation}: ")?;
                }
                write!(f, "Twilio HTTP {}", meta.status.as_u16())?;
                match document {
                    Ok(document) => {
                        if let Some(code) = document.code {
                            write!(f, " (code {code})")?;
                        }
                        Ok(())
                    }
                    Err(problem) => write!(f, " (error document: {problem:?})"),
                }
            }
        }
    }
}

impl std::error::Error for Error {}

pub(crate) async fn send<T>(prepared: Prepared<T>) -> Result<Response<T>, Error> {
    let received = prepared.receive().await?;
    let meta = ResponseMeta::capture(&received);
    if !meta.status.is_success() {
        return Err(Error::Provider {
            meta: Box::new(meta),
            document: read_error(received).await,
        });
    }
    match received.decode().await {
        Ok(value) => Ok(Response { value, meta }),
        Err(error) => Err(Error::Concord {
            error,
            meta: Some(Box::new(meta)),
        }),
    }
}

async fn read_error<T>(received: Received<T>) -> Result<ProviderError, ErrorDocumentProblem> {
    let head = received.head();
    <Json as Decode<ProviderError>>::check_content_type(&head)
        .map_err(|_| ErrorDocumentProblem::ContentType)?;
    <Json as Decode<ProviderError>>::context(&head).map_err(|_| ErrorDocumentProblem::Decode)?;
    let bytes = received
        .bytes_limited(MAX_ERROR_BODY)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::Limit => ErrorDocumentProblem::TooLarge,
            _ => ErrorDocumentProblem::Read,
        })?;
    <Json as Decode<ProviderError>>::decode_body((), bytes)
        .map_err(|_| ErrorDocumentProblem::Decode)
}
