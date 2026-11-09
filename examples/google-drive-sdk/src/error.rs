use std::fmt;

use concord::reqwest::StatusCode;
use concord::{IgnoreContentType, Json, Operation, Received};
use serde::Deserialize;

/// Status, version, response headers (Location, Range, Retry-After, request identifiers) and the
/// generated operation identity, captured before consuming a body: Concord's own type.
/// Header values are accessible explicitly; Debug prints only their names.
pub use concord::ResponseMeta;

/// A successful value and its HTTP metadata.
#[derive(Debug)]
pub struct Response<T> {
    /// Typed provider value.
    pub value: T,
    /// Preserved status and headers.
    pub meta: ResponseMeta,
}

/// One reason in Drive's error document. Values may quote submitted data.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
#[sensitive]
pub struct ProviderReason {
    /// Error domain.
    pub domain: Option<String>,
    /// Machine-readable reason, such as `rateLimitExceeded`.
    pub reason: String,
    /// Provider's explanatory message, unredacted.
    pub message: Option<String>,
    /// Parameter/header name, if supplied.
    pub location: Option<String>,
    /// Kind of location, if supplied.
    pub location_type: Option<String>,
}

/// Drive's JSON `error` object. Inspect explicitly; diagnostics withhold provider text.
#[derive(Clone, Deserialize)]
pub struct ProviderError {
    /// Provider's numeric code; the actual HTTP status remains in [`ResponseMeta`].
    pub code: u16,
    /// Explanatory text, unredacted.
    pub message: String,
    /// Detailed reasons, when supplied.
    #[serde(default)]
    pub errors: Vec<ProviderReason>,
    /// Google RPC-style status, when supplied.
    pub status: Option<String>,
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("code", &self.code)
            .field("reasons", &self.errors.len())
            .finish_non_exhaustive()
    }
}

/// Why a non-success response had no usable Drive error document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentProblem {
    /// Body was not a JSON Drive error document.
    Malformed,
    /// Body exceeded the configured error limit.
    TooLarge,
    /// Body transfer failed.
    Read,
}

/// SDK failure classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Rejected before sending anything; static explanation contains no submitted value.
    InvalidInput(&'static str),
    /// Contradictory or unsafe provider protocol state.
    Protocol(&'static str),
    /// Non-success provider HTTP response.
    Provider,
    /// A session status or transfer returned 404; the session has expired.
    SessionExpired,
    /// Request construction, transport, metadata check, decode, or body-limit failure.
    Concord,
}

/// A typed failure retaining operation identity, metadata, and provider details where available.
#[derive(concord::Debug)]
#[sensitive]
pub struct Error {
    #[public]
    kind: ErrorKind,
    #[public]
    meta: Option<Box<ResponseMeta>>,
    #[public]
    operation: Option<Operation>,
    provider: Option<Box<ProviderError>>,
    #[public]
    document_problem: Option<DocumentProblem>,
    source: Option<concord::Error>,
}

impl Error {
    fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            meta: None,
            operation: None,
            provider: None,
            document_problem: None,
            source: None,
        }
    }
    pub(crate) fn invalid(message: &'static str) -> Self {
        Self::new(ErrorKind::InvalidInput(message))
    }
    pub(crate) fn protocol(message: &'static str, meta: Option<ResponseMeta>) -> Self {
        let mut error = Self::new(ErrorKind::Protocol(message));
        error.operation = meta.as_ref().and_then(ResponseMeta::operation);
        error.meta = meta.map(Box::new);
        error
    }
    pub(crate) fn with_meta(mut self, meta: ResponseMeta) -> Self {
        self.operation = meta.operation();
        self.meta = Some(Box::new(meta));
        self
    }
    /// Failure category.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
    /// Response information, even if decoding or reading failed.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        self.meta.as_deref()
    }
    /// Declared operation, when a request was prepared/received.
    pub fn operation(&self) -> Option<Operation> {
        self.operation
    }
    /// Typed Drive error object, without losing it to a string conversion.
    pub fn provider(&self) -> Option<&ProviderError> {
        self.provider.as_deref()
    }
    /// Error-document failure, if the non-success body was unusable.
    pub fn document_problem(&self) -> Option<DocumentProblem> {
        self.document_problem
    }
    /// Underlying Concord error, for kind and codec/transport diagnostics.
    pub fn concord(&self) -> Option<&concord::Error> {
        self.source.as_ref()
    }

    pub(crate) async fn provider_response<T>(
        received: Received<T>,
        limit: usize,
        session: bool,
    ) -> Self {
        #[derive(Deserialize)]
        struct Document {
            error: ProviderError,
        }
        // Drive error documents are decoded regardless of the advertised media type.
        let (meta, document) = received
            .document::<IgnoreContentType<Json>, Document>(limit)
            .await;
        let kind = if session && meta.status() == StatusCode::NOT_FOUND {
            ErrorKind::SessionExpired
        } else {
            ErrorKind::Provider
        };
        let mut error = Self::new(kind).with_meta(meta);
        match document {
            Ok(document) => error.provider = Some(Box::new(document.error)),
            Err(source) if source.kind() == concord::ErrorKind::Decode => {
                error.document_problem = Some(DocumentProblem::Malformed);
            }
            Err(source) => {
                error.document_problem = Some(if source.kind() == concord::ErrorKind::Limit {
                    DocumentProblem::TooLarge
                } else {
                    DocumentProblem::Read
                });
                error.source = Some(source);
            }
        }
        error
    }
}

impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let mut error = Self::new(ErrorKind::Concord);
        error.operation = source.operation();
        error.meta = source.head_meta().map(Box::new);
        error.source = Some(source);
        error
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Google Drive {:?}", self.kind)?;
        if let Some(operation) = self.operation {
            write!(f, " in {operation}")?;
        }
        if let Some(meta) = &self.meta {
            write!(f, " (HTTP {})", meta.status())?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}
