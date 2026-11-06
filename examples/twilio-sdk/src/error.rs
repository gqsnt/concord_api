//! Response metadata and bounded provider error decoding.

use std::fmt;

use concord::{ErrorKind, Json, Operation, Prepared};
use serde::Deserialize;

/// Maximum retained bytes of a non-success response body (64 KiB).
pub const MAX_ERROR_BODY: usize = 64 * 1024;

/// Metadata captured before consuming any response body, including failed decodes: Concord's
/// own type. `status()` is the actual HTTP status, independent of any status claimed in an
/// error document; `headers()` are all response headers, unredacted (including duplicate
/// values), for provider request identifiers and retry advice when present (this SDK does not
/// require any particular request-id header or invent one if absent); `operation()` is the
/// identity supplied by Concord's declaration. `Debug` prints header names only.
pub use concord::ResponseMeta;

/// One complete resource and the response metadata that accompanied it.
#[derive(Clone)]
pub struct Response<T> {
    /// The decoded resource; its fields can contain personal data.
    pub value: T,
    /// Metadata of the successful HTTP response.
    pub meta: ResponseMeta,
}

/// Shows the value through its own `Debug` (for [`crate::Message`]: SID, status and error code,
/// without addresses, text or explanations) and the metadata.
impl<T: fmt::Debug> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("value", &self.value)
            .field("meta", &self.meta)
            .finish()
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

/// The SDK's error for building or sending an operation (every [`crate::Twilio`] operation
/// method returns it). An HTTP 200 message with `status: failed` is instead successful data.
///
/// Only [`crate::Twilio::with_base_url`] returns Concord's own `concord::Error` (a `Build`
/// error for an unusable base URL); `?` converts it into [`TwilioError::Concord`].
#[derive(Debug)]
#[non_exhaustive]
pub enum TwilioError {
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
    /// An empty message SID: refused before building the request, which would address
    /// `Messages/.json`. Nothing was sent.
    EmptySid {
        /// The operation that was refused (`TwilioApi.get_message`).
        operation: Operation,
    },
}

/// The former name of [`TwilioError`], kept so existing code compiles; hidden from the docs so
/// that no signature reads as an ambiguous `Error`.
#[doc(hidden)]
pub type Error = TwilioError;

impl TwilioError {
    /// Response metadata, or `None` if the request failed before receiving a head.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Provider { meta, .. } => Some(meta),
            Self::Concord { meta, .. } => meta.as_deref(),
            Self::EmptySid { .. } => None,
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
        if let Self::EmptySid { operation } = self {
            return Some(*operation);
        }
        self.meta()
            .and_then(ResponseMeta::operation)
            .or_else(|| self.concord().and_then(concord::Error::operation))
    }
}

impl From<concord::Error> for TwilioError {
    fn from(error: concord::Error) -> Self {
        let meta = error.meta().map(Box::new);
        Self::Concord { error, meta }
    }
}

impl fmt::Display for TwilioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord { error, .. } => fmt::Display::fmt(error, f),
            Self::Provider { meta, document } => {
                if let Some(operation) = meta.operation() {
                    write!(f, "{operation}: ")?;
                }
                write!(f, "Twilio HTTP {}", meta.status().as_u16())?;
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
            Self::EmptySid { operation } => {
                write!(f, "{operation}: empty message SID (nothing sent)")
            }
        }
    }
}

impl std::error::Error for TwilioError {}

pub(crate) async fn send<T>(prepared: Prepared<T>) -> Result<Response<T>, TwilioError> {
    let received = prepared.receive().await?;
    if !received.status().is_success() {
        let (meta, document) = received
            .document::<Json, ProviderError>(MAX_ERROR_BODY)
            .await;
        let document = document.map_err(|error| match error.kind() {
            ErrorKind::Metadata => ErrorDocumentProblem::ContentType,
            ErrorKind::Limit => ErrorDocumentProblem::TooLarge,
            ErrorKind::Decode => ErrorDocumentProblem::Decode,
            _ => ErrorDocumentProblem::Read,
        });
        return Err(TwilioError::Provider {
            meta: Box::new(meta),
            document,
        });
    }
    let (value, meta) = received.decode_with_meta().await?;
    Ok(Response { value, meta })
}
