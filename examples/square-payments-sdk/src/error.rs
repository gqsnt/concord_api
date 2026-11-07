use super::*;

/// SDK call failure. Provider strings and response header values are withheld from diagnostics.
#[derive(Debug)]
pub enum Error {
    /// Request build, transport, success metadata, decode or body-limit failure.
    Concord(concord::Error),
    /// Any non-2xx status, even if its error document cannot be read.
    Provider(Box<ProviderError>),
}

impl From<concord::Error> for Error {
    fn from(value: concord::Error) -> Self {
        Self::Concord(value)
    }
}

impl Error {
    /// Whether a failed create may have taken effect.
    ///
    /// Conservatively true for a create's transport/unreadable success, 5xx or
    /// redirect. This does not grant permission to change a replay's key/body.
    pub fn create_outcome_unknown(&self) -> bool {
        match self {
            Self::Concord(error) => {
                error.operation().and_then(|op| op.method()) == Some("POST")
                    && error.delivery() != concord::Delivery::NotSent
            }
            Self::Provider(error) => {
                error.meta.operation().and_then(|op| op.method()) == Some("POST")
                    && (error.meta.status().is_server_error()
                        || error.meta.status().is_redirection())
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(error) => fmt::Display::fmt(error, f),
            Self::Provider(error) => write!(f, "Square HTTP {}", error.meta.status().as_u16()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Concord(error) => Some(error),
            Self::Provider(_) => None,
        }
    }
}

/// An HTTP failure whose status stays authoritative when its document is invalid.
#[derive(Debug)]
pub struct ProviderError {
    /// Response status, operation and headers. Explicit header access is unredacted.
    pub meta: concord::ResponseMeta,
    /// Strict JSON error document or the bounded-read/media/decode failure.
    /// Explicit document access can disclose provider-echoed request values.
    pub document: Result<ErrorDocument, concord::Error>,
}

impl ProviderError {
    /// Square refused reuse of a key with different request parameters.
    /// Unknown codes remain inspectable through [`Self::document`].
    pub fn is_idempotency_mismatch(&self) -> bool {
        self.document.as_ref().is_ok_and(|document| {
            document
                .errors
                .iter()
                .any(|error| error.code == "IDEMPOTENCY_KEY_REUSED")
        })
    }
}

/// Square's error envelope, retaining every entry and unknown category/code strings.
#[derive(Debug, Deserialize)]
pub struct ErrorDocument {
    /// Provider errors. An absent or wrongly shaped array is a document decode failure.
    pub errors: Vec<SquareError>,
}

/// A provider error. Debug withholds all strings, including code/category, which are untrusted.
#[derive(Deserialize)]
pub struct SquareError {
    /// Provider error category; no exhaustive SDK enum.
    pub category: String,
    /// Specific error code; no exhaustive SDK enum.
    pub code: String,
    /// Provider detail, possibly echoing secrets; explicitly accessed only.
    pub detail: Option<String>,
    /// Provider field name, possibly echoing request values.
    pub field: Option<String>,
}

impl fmt::Debug for SquareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SquareError(<redacted>)")
    }
}
