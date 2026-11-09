use serde::Deserialize;
use std::fmt;

/// HTTP failure class; body fields never override the actual HTTP status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpClass {
    /// 400, 405, 406, 408 or 413 request failure.
    Request,
    /// 401: invalid/missing credential.
    Authentication,
    /// 403: insufficient permission.
    Permission,
    /// 404: resource or route unavailable.
    NotFound,
    /// 409: duplicate or conflicting operation.
    Conflict,
    /// 422: semantic validation failure (also used for code 704).
    Validation,
    /// 429: provider throttling.
    RateLimited,
    /// Any 5xx, including idempotency-store unavailability.
    Server,
    /// Other non-success, including an unfollowed redirect.
    Other,
}

impl HttpClass {
    pub(crate) fn of(status: u16) -> Self {
        match status {
            400 | 405 | 406 | 408 | 413 => Self::Request,
            401 => Self::Authentication,
            403 => Self::Permission,
            404 => Self::NotFound,
            409 => Self::Conflict,
            422 => Self::Validation,
            429 => Self::RateLimited,
            500..=599 => Self::Server,
            _ => Self::Other,
        }
    }
}

/// Bounded provider error document. Its strings are untrusted and explicitly accessible only.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
#[sensitive]
pub struct Problem {
    /// Provider's reported HTTP status (actual metadata remains authoritative).
    pub status: u16,
    /// Provider diagnostic code, for example 703 or 704.
    pub error_code: String,
    /// Provider diagnostic category.
    pub error_type: String,
    /// Provider diagnostic text, potentially sensitive.
    pub message: String,
    /// Reference for reconciliation, if available.
    pub psp_reference: Option<String>,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Adyen error document (details withheld)")
    }
}

/// A non-success HTTP response, even when its body cannot be decoded.
#[derive(Debug)]
pub struct ProviderFailure {
    /// Actual status, headers (values accessible explicitly) and operation identity.
    pub meta: concord::ResponseMeta,
    /// HTTP class independent of error-document health.
    pub class: HttpClass,
    /// Parsed document, or the precise bounded-read/media/decode failure.
    pub document: Result<Problem, Box<concord::Error>>,
    /// Whether the final head explicitly permits retry and has a valid Retry-After, if present.
    /// Retrying still requires the same key/request and an unexpired ledger entry.
    pub transient: bool,
    /// Application attempt accounting.
    pub retry: concord_retry::Report,
}

/// Why a payment must not be replaced with a fresh submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconciliationReason {
    /// A previously registered key was supplied with different request bytes or context.
    ChangedRequest,
    /// The conservative guarantee window expired, including while waiting to retry.
    GuaranteeExpired,
    /// The key was explicitly invalidated after provider assurance was lost.
    GuaranteeRevoked,
    /// Success did not echo exactly one identical idempotency key, or its document describes a
    /// different request (a capture of another payment, amount, merchant account or reference).
    GuaranteeNotConfirmed,
    /// Provider error 703: idempotency storage is unavailable after bounded retries.
    GuaranteeUnavailable,
    /// Provider error 704: already processed or still in progress after bounded retries.
    InProgress,
    /// Another local call with the identical key is currently in flight.
    InFlight,
    /// No trustworthy final payment response arrived; exact replay may still be possible.
    UncertainDelivery,
    /// Total attempt budget expired; a cancelled attempt may still have executed remotely.
    Deadline,
}

/// Reconciliation evidence. No formatting exposes request keys, bodies or provider text.
#[derive(Debug)]
pub struct Reconciliation {
    /// Reason for stopping.
    pub reason: ReconciliationReason,
    /// Head retained when idempotency confirmation fails before body consumption.
    pub meta: Option<concord::ResponseMeta>,
    /// Final provider failure when available, including malformed error-document evidence.
    pub provider: Option<Box<ProviderFailure>>,
    /// Transport or success-body failure, with response head when one arrived.
    pub cause: Option<Box<concord::Error>>,
    /// Retry accounting when any attempt loop ran.
    pub retry: Option<concord_retry::Report>,
}

/// SDK errors. Debug and Display withhold credentials, keys, request and response bodies.
#[derive(Debug)]
pub enum Error {
    /// Invalid local input; no request was dispatched.
    InvalidInput(&'static str),
    /// Ledger capacity reached. Entries are never silently evicted to permit unsafe reuse.
    LedgerFull,
    /// Ledger lock was poisoned; fail closed.
    PolicyUnavailable,
    /// Concord preparation failure; no I/O occurred.
    Concord(Box<concord::Error>),
    /// A provider HTTP error with authoritative metadata.
    Provider(Box<ProviderFailure>),
    /// Reconcile using the original key/reference; never silently issue a fresh payment.
    Reconciliation(Box<Reconciliation>),
}

impl Error {
    pub(crate) fn reconcile(reason: ReconciliationReason) -> Self {
        Self::Reconciliation(Box::new(Reconciliation {
            reason,
            meta: None,
            provider: None,
            cause: None,
            retry: None,
        }))
    }

    /// Reconciliation evidence, if this failure needs it.
    pub fn reconciliation(&self) -> Option<&Reconciliation> {
        match self {
            Self::Reconciliation(e) => Some(e),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(Box::new(e))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(label) => f.write_str(label),
            Self::LedgerFull => {
                f.write_str("idempotency ledger is full; preserve it and reconcile")
            }
            Self::PolicyUnavailable => f.write_str("idempotency policy is unavailable"),
            Self::Concord(_) => f.write_str("request preparation failed"),
            Self::Provider(e) => {
                write!(f, "Adyen HTTP {} ({:?})", e.meta.status().as_u16(), e.class)
            }
            Self::Reconciliation(e) => {
                write!(f, "payment requires reconciliation ({:?})", e.reason)
            }
        }
    }
}

impl std::error::Error for Error {}
