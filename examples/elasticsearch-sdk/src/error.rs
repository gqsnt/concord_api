//! The SDK's error type and the Elasticsearch error documents it carries.

use std::fmt;

use concord::{Delivery, ResponseMeta};
use serde::Deserialize;

use crate::bulk::BulkReport;

/// Every failure of this SDK.
///
/// `Display` and `Debug` never print a credential, a document source, a PIT id, sort values or
/// an Elasticsearch error `reason` (which can quote document content): they show static text,
/// statuses, positions, counts and Elasticsearch error *types*.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// An input this SDK refuses before anything is sent.
    Invalid(Invalid),
    /// Concord's own error: the request could not be built, the transport failed, or a 2xx
    /// response was refused by its format, too long or malformed. `delivery()` says whether the
    /// request may have reached Elasticsearch.
    Concord(Box<concord::Error>),
    /// Elasticsearch answered with a non-2xx status. The status is authoritative whether its
    /// error document is typed, absent, malformed or oversized.
    Provider(Box<ProviderError>),
    /// Conditional indexing was rejected with HTTP 409. Read the current document and let the
    /// caller resolve the conflict; do not automatically repeat the write with a newer token.
    VersionConflict(Box<ProviderError>),
    /// A 2xx response that contradicts the request or the documented contract (a bulk item
    /// that cannot be matched with its operation, a PIT page without its PIT id or sort values).
    Protocol(Protocol),
    /// A bulk request whose response reports at least one failed operation. The report holds
    /// every operation's outcome; [`BulkReport::resubmit`] builds a request of failed
    /// operations only.
    PartialBulk(Box<BulkReport>),
    /// A search page that Elasticsearch marked incomplete (`timed_out`, or `_shards.failed`
    /// greater than zero). Continuing after it could skip hits, so the traversal stops.
    PartialSearch {
        /// The page's `_shards.failed`.
        shards_failed: u64,
        /// The page's `timed_out`.
        timed_out: bool,
    },
    /// A point-in-time traversal reached its page cap while Elasticsearch had more pages.
    PageCap {
        /// The number of pages read.
        pages: u64,
    },
}

impl Error {
    /// Whether a **write** request may have been applied, wholly or in part, without this SDK
    /// being able to tell which operations: no response head arrived (Concord's
    /// [`Delivery::Unknown`]), Elasticsearch answered 5xx, or it answered 2xx with a body that
    /// was refused, too long, cut off, malformed or contradicts the request ([`Error::Protocol`]).
    ///
    /// Such a request must not be resubmitted blindly. A [`PartialBulk`](Error::PartialBulk)
    /// is **not** uncertain: every operation's outcome is known. A conditional write's timeout
    /// is uncertain too, even if a 2xx head arrived; no write is automatically resent.
    pub fn is_uncertain(&self) -> bool {
        match self {
            Error::Concord(error) => match error.delivery() {
                Delivery::NotSent => false,
                Delivery::Unknown => true,
                // Answered: only a 2xx can reach Concord's error here (the send policy turns a
                // non-2xx into `Provider`), and a 2xx means Elasticsearch executed the request.
                _ => error.head().is_some_and(|head| head.status().is_success()),
            },
            Error::Provider(provider) => provider.status().is_server_error(),
            Error::Protocol(_) => true,
            _ => false,
        }
    }

    /// The HTTP status of an Elasticsearch error answer (provider error or version conflict).
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Provider(provider) | Error::VersionConflict(provider) => {
                Some(provider.status().as_u16())
            }
            _ => None,
        }
    }

    /// Concord's error, when this is one.
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Error::Concord(error) => Some(error),
            _ => None,
        }
    }

    /// The bulk report of a partially failed bulk request.
    pub fn partial_bulk(&self) -> Option<&BulkReport> {
        match self {
            Error::PartialBulk(report) => Some(report),
            _ => None,
        }
    }

    /// The bulk report of a partially failed bulk request, by value.
    pub fn into_partial_bulk(self) -> Result<BulkReport, Error> {
        match self {
            Error::PartialBulk(report) => Ok(*report),
            other => Err(other),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Invalid(invalid) => write!(f, "invalid input: {invalid}"),
            // Concord's `Display` is redacted (endpoint, static label, numbers).
            Error::Concord(error) => error.fmt(f),
            Error::Provider(provider) => provider.fmt(f),
            Error::VersionConflict(provider) => {
                write!(f, "conditional indexing conflict: {provider}")
            }
            Error::Protocol(protocol) => write!(f, "unexpected Elasticsearch response: {protocol}"),
            Error::PartialBulk(report) => write!(
                f,
                "bulk request partially failed: {} of {} operations failed",
                report.failed().count(),
                report.items().len()
            ),
            Error::PartialSearch {
                shards_failed,
                timed_out,
            } => write!(
                f,
                "incomplete search page ({shards_failed} shard failures, timed_out: {timed_out})"
            ),
            Error::PageCap { pages } => {
                write!(
                    f,
                    "page cap reached after {pages} pages while more remained"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Error::Concord(Box::new(error))
    }
}

/// An input refused before anything is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Invalid {
    /// An index name that is empty, starts with `_`, or contains a character this slice refuses
    /// (`,`, `*`, `/`, `\`, whitespace or a control character).
    IndexName,
    /// An empty document id.
    DocumentId,
    /// A document or partial document that does not serialize to a JSON object.
    DocumentNotObject,
    /// A document that could not be serialized.
    DocumentEncoding,
    /// A sequence number / primary term pair with a zero primary term.
    DocumentVersion,
    /// A single document source exceeds the bounded JSON body size.
    DocumentTooLarge,
    /// A document whose JSON text contains a line break (a raw JSON value): it would split its
    /// NDJSON line.
    DocumentLineBreak,
    /// The bulk request would exceed its operation limit.
    TooManyOperations,
    /// The bulk request would exceed its byte limit.
    TooManyBytes,
    /// Bulk limits of zero or above the hard caps.
    BulkLimits,
    /// A bulk request without operations.
    EmptyBulk,
    /// A page size of zero or above 10,000.
    PageSize,
    /// A search without sort clauses, or a sort clause that is not a one-field object.
    Sort,
    /// A query that is not a JSON object.
    Query,
    /// An API key that is empty or not visible ASCII.
    ApiKey,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Invalid::IndexName => "index name refused",
            Invalid::DocumentId => "empty document id",
            Invalid::DocumentNotObject => "a document must serialize to a JSON object",
            Invalid::DocumentEncoding => "a document could not be serialized",
            Invalid::DocumentVersion => "a document primary term must be positive",
            Invalid::DocumentTooLarge => "a document exceeds the source byte limit",
            Invalid::DocumentLineBreak => "a document's JSON text contains a line break",
            Invalid::TooManyOperations => "the bulk request would exceed its operation limit",
            Invalid::TooManyBytes => "the bulk request would exceed its byte limit",
            Invalid::BulkLimits => "bulk limits must be positive and within the hard caps",
            Invalid::EmptyBulk => "a bulk request needs at least one operation",
            Invalid::PageSize => "the page size must be between 1 and 10000",
            Invalid::Sort => "a search_after traversal needs one-field sort clauses",
            Invalid::Query => "a query must be a JSON object",
            Invalid::ApiKey => "an API key must be non-empty visible ASCII",
        })
    }
}

/// A 2xx response that contradicts the request or the documented contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Protocol {
    /// A document read or write response names a different id than requested.
    DocumentId,
    /// A document read's HTTP status contradicts its `found` field.
    DocumentStatus,
    /// The bulk response has a different number of items than operations were submitted, so
    /// items cannot be matched with operations by position.
    ItemCount {
        /// Operations submitted.
        submitted: usize,
        /// Items returned.
        returned: usize,
    },
    /// The item at this position names another action than the submitted operation.
    ItemAction {
        /// 0-based position.
        position: usize,
    },
    /// The item at this position names another document id than the submitted operation.
    ItemId {
        /// 0-based position.
        position: usize,
    },
    /// The response's `errors` flag contradicts its items.
    ErrorsFlag,
    /// A PIT search response without a (non-empty) `pit_id`.
    MissingPitId,
    /// An open-PIT response with an empty `id`.
    EmptyPitId,
    /// A hit without a non-empty `sort` array.
    MissingSort {
        /// 0-based position of the hit in its page.
        position: usize,
    },
    /// A page with more hits than requested.
    TooManyHits {
        /// The requested page size.
        requested: u32,
        /// Hits returned.
        returned: usize,
    },
    /// A page whose last sort values equal the `search_after` it was requested with: the
    /// traversal would not advance.
    StalledCursor,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::DocumentId => f.write_str("the document response names another id"),
            Protocol::DocumentStatus => f.write_str("the document status contradicts found"),
            Protocol::ItemCount {
                submitted,
                returned,
            } => write!(f, "{returned} bulk items for {submitted} operations"),
            Protocol::ItemAction { position } => {
                write!(f, "bulk item {position} names another action")
            }
            Protocol::ItemId { position } => {
                write!(f, "bulk item {position} names another document id")
            }
            Protocol::ErrorsFlag => f.write_str("the bulk `errors` flag contradicts its items"),
            Protocol::MissingPitId => f.write_str("a PIT search response without pit_id"),
            Protocol::EmptyPitId => f.write_str("an empty PIT id"),
            Protocol::MissingSort { position } => {
                write!(f, "hit {position} has no sort values")
            }
            Protocol::TooManyHits {
                requested,
                returned,
            } => write!(f, "{returned} hits for a page size of {requested}"),
            Protocol::StalledCursor => f.write_str("the search_after cursor did not advance"),
        }
    }
}

/// A non-2xx answer: its head and the error document, if one could be read.
pub struct ProviderError {
    meta: ResponseMeta,
    document: Result<ErrorDocument, concord::Error>,
}

impl ProviderError {
    pub(crate) fn new(meta: ResponseMeta, document: Result<ErrorDocument, concord::Error>) -> Self {
        Self { meta, document }
    }

    /// The HTTP status.
    pub fn status(&self) -> concord::reqwest::StatusCode {
        self.meta.status()
    }

    /// The response head (status, version, headers). Its `Debug` prints header names only.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The error document, or why it could not be read (absent, not JSON, oversized, malformed).
    pub fn document(&self) -> Result<&ErrorDocument, &concord::Error> {
        self.document.as_ref()
    }

    /// The error's `type` (`index_not_found_exception`, ...), when the document had one.
    pub fn error_type(&self) -> Option<&str> {
        match self.document.as_ref().ok()?.error.as_ref()? {
            ErrorField::Cause(cause) => Some(cause.kind()),
            ErrorField::Text(_) => None,
        }
    }

    /// Whether Elasticsearch refused the request as a whole before executing it: this SDK reads
    /// every 4xx answer to a bulk request that way (Elasticsearch reports per-operation failures
    /// inside a 2xx response instead).
    pub fn is_rejection(&self) -> bool {
        self.status().is_client_error()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Elasticsearch answered HTTP {}", self.status().as_u16())?;
        if let Some(kind) = self.error_type() {
            write!(f, " ({kind})")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderError")
            .field("meta", &self.meta)
            .field("document", &self.document)
            .finish()
    }
}

/// An Elasticsearch error document: `{"error": {...}, "status": 400}`.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct ErrorDocument {
    /// The error: a cause object, or (in some answers) a bare string.
    #[serde(default)]
    pub error: Option<ErrorField>,
    /// The HTTP status repeated in the body, when present.
    #[serde(default)]
    pub status: Option<u16>,
}

/// The `error` member of an error document.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ErrorField {
    /// An error cause object (`type`, `reason`, ...).
    Cause(ErrorCause),
    /// A bare error string.
    Text(String),
}

impl fmt::Debug for ErrorField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorField::Cause(cause) => f.debug_tuple("Cause").field(cause).finish(),
            ErrorField::Text(text) => write!(f, "Text(<{} bytes withheld>)", text.len()),
        }
    }
}

/// An Elasticsearch error cause: the `error` of a failed bulk item or of an error document.
///
/// `Debug` shows the type only: the `reason` can quote document content (the bulk API's
/// `include_source_on_error` defaults to `true`); read it deliberately with [`reason`](Self::reason).
#[derive(Clone, Deserialize)]
pub struct ErrorCause {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    root_cause: Vec<ErrorCause>,
    #[serde(default)]
    caused_by: Option<Box<ErrorCause>>,
}

impl ErrorCause {
    /// The error type (`document_missing_exception`, `es_rejected_execution_exception`, ...).
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The human-readable reason, **unredacted**: it can quote document content.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// The root causes Elasticsearch reported.
    pub fn root_cause(&self) -> &[ErrorCause] {
        &self.root_cause
    }

    /// The nested cause, if any.
    pub fn caused_by(&self) -> Option<&ErrorCause> {
        self.caused_by.as_deref()
    }
}

impl fmt::Debug for ErrorCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorCause")
            .field("type", &self.kind)
            .field("reason", &self.reason.as_ref().map(|_| Withheld))
            .finish_non_exhaustive()
    }
}

struct Withheld;

impl fmt::Debug for Withheld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<withheld>")
    }
}
