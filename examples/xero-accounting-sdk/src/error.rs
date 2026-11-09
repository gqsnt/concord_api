//! Value-free diagnostics and explicitly accessed provider documents.
use serde::Deserialize;
use std::{fmt, sync::Arc, time::Duration};

/// Accounting HTTP error classes documented by Xero, plus an unknown-status fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// HTTP 400.
    Validation,
    /// HTTP 401; reauthorization may be necessary.
    Unauthorized,
    /// HTTP 403.
    Forbidden,
    /// HTTP 404.
    NotFound,
    /// HTTP 412.
    Precondition,
    /// HTTP 429.
    RateLimited,
    /// HTTP 500.
    Internal,
    /// HTTP 501.
    NotImplemented,
    /// HTTP 503, including an offline organisation.
    Unavailable,
    /// Another non-success status.
    Other,
}
impl Class {
    pub(crate) fn status(status: u16) -> Self {
        match status {
            400 => Self::Validation,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            412 => Self::Precondition,
            429 => Self::RateLimited,
            500 => Self::Internal,
            501 => Self::NotImplemented,
            503 => Self::Unavailable,
            _ => Self::Other,
        }
    }
}

/// A bounded JSON error document. Fields are disclosed only by deliberate access.
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub struct Problem {
    /// Accounting error number, if supplied.
    #[serde(rename = "ErrorNumber")]
    pub number: Option<u32>,
    /// Accounting exception type, if supplied.
    #[serde(rename = "Type")]
    pub exception_type: Option<String>,
    /// Provider text; may contain personal information.
    #[serde(rename = "Message")]
    pub message: Option<String>,
    /// OAuth error code, if supplied.
    pub error: Option<String>,
    /// OAuth error text; may contain secrets.
    pub error_description: Option<String>,
    /// Per-resource validation data, deliberately unformatted.
    #[serde(rename = "Elements")]
    pub elements: Option<serde_json::Value>,
}

/// The HTTP failure remains authoritative even when its body cannot be decoded.
#[derive(Debug)]
pub struct ProviderFailure {
    /// Classification of the HTTP status.
    pub class: Class,
    /// Owned head; Concord redacts header values in Debug.
    pub meta: concord::ResponseMeta,
    /// Typed document, or the exact safe Concord read/decode failure.
    pub document: Result<Problem, Box<concord::Error>>,
    /// Strict Retry-After interpretation, including absent versus malformed/repeated.
    pub retry_after: Result<Option<Duration>, concord_retry::http::RetryAfterError>,
}

/// Distinct local and remote failure stages.
#[derive(Debug)]
pub enum ErrorKind {
    /// A value was rejected before I/O. The label is static.
    Input(&'static str),
    /// A tenant-bound value was used in another tenant.
    TenantMismatch,
    /// The durable token journal needs explicit recovery or reauthorization.
    RecoveryRequired,
    /// Another process/session currently owns the journal lock.
    StoreBusy,
    /// A filesystem operation failed; paths and OS messages are withheld.
    Storage,
    /// Credential acquisition failed; ordinary formatting never traverses the inner error.
    Authentication(Arc<Error>),
    /// A Concord build, transport, media, decoding or size failure.
    Concord(Box<concord::Error>),
    /// An HTTP error, including a potentially unreadable error body.
    Provider(Box<ProviderFailure>),
    /// Invalid or inconsistent provider success semantics.
    Contract(&'static str),
    /// A 2xx invoice envelope reported validation errors.
    InvoiceValidation,
    /// Process-local concurrency or cooldown admission refused.
    Admission(concord_ratelimit::Error),
    /// A bounded read retry had no completed result.
    Deadline,
}

/// SDK error. Debug/Display never render body, token, tenant, identifier or OS path values.
pub struct Error {
    kind: ErrorKind,
    uncertain: bool,
}
impl Error {
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            uncertain: false,
        }
    }
    /// Inspect the failure stage and explicit diagnostic payload.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
    /// A create may have committed; reconcile within this tenant before any manual replay.
    pub fn write_uncertain(&self) -> bool {
        self.uncertain
    }
    pub(crate) fn uncertain(mut self) -> Self {
        self.uncertain = true;
        self
    }
    pub(crate) fn input(label: &'static str) -> Self {
        Self::new(ErrorKind::Input(label))
    }
    /// The accounting HTTP classification, if a response was received and refused.
    pub fn class(&self) -> Option<Class> {
        match &self.kind {
            ErrorKind::Provider(p) => Some(p.class),
            _ => None,
        }
    }
    /// The underlying Concord error, including head and delivery evidence.
    pub fn concord_error(&self) -> Option<&concord::Error> {
        match &self.kind {
            ErrorKind::Concord(e) => Some(e),
            _ => None,
        }
    }
}
impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::new(ErrorKind::Concord(Box::new(e)))
    }
}
impl From<concord_ratelimit::Error> for Error {
    fn from(e: concord_ratelimit::Error) -> Self {
        Self::new(ErrorKind::Admission(e))
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match &self.kind {
            ErrorKind::Input(s) | ErrorKind::Contract(s) => s,
            ErrorKind::TenantMismatch => "tenant binding mismatch",
            ErrorKind::RecoveryRequired => "token journal requires recovery or reauthorization",
            ErrorKind::StoreBusy => "token journal is locked",
            ErrorKind::Storage => "token journal persistence failed",
            ErrorKind::Authentication(_) => "credential acquisition failed",
            ErrorKind::Concord(_) => "invoice request or response failed",
            ErrorKind::Provider(_) => "provider refused the request",
            ErrorKind::InvoiceValidation => "invoice validation failed",
            ErrorKind::Admission(_) => "local rate admission refused",
            ErrorKind::Deadline => "read retry budget expired",
        })
    }
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("stage", &self.to_string())
            .field("write_uncertain", &self.uncertain)
            .finish()
    }
}
impl std::error::Error for Error {}
