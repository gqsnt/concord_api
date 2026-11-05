//! The SDK's error type. Underlying Concord, stream and pagination errors are wrapped, never
//! stringified; `Display`/`Debug` show operation labels, status codes, error codes, static
//! reasons and counts, never a token, URL, locator, query text or record value.

use std::fmt;

use concord::reqwest::StatusCode;
use concord::stream::{OpenError, OpenErrorKind, ReadError};
use concord::{Decode, ErrorKind, Json};

use crate::bulk::JobState;
use crate::model::{ApiErrorBody, ApiErrorDoc};

/// The most bytes of an error document read into memory; a longer one is not decoded.
pub const MAX_ERROR_BODY: usize = 64 * 1024;

/// A failure of an SDK operation.
#[non_exhaustive]
pub enum SfError {
    /// A declared operation failed inside Concord: the request could not be built, the transport
    /// failed, a 2xx head was refused (`Metadata`) or a 2xx body did not decode. (Non-2xx
    /// statuses become [`SfError::Api`].)
    Concord(concord::Error),
    /// Salesforce answered a non-2xx status, with its error document when one could be read.
    Api(Box<ApiFailure>),
    /// A response contradicts the documented protocol (a missing next link, a changed dialect,
    /// a record count that disagrees with `Sforce-NumberOfRecords`, ...).
    Protocol(Box<ProtocolError>),
    /// A link returned by Salesforce was refused before use.
    Link(LinkError),
    /// A CSV body's head was refused before it was read (the response is kept unread).
    Open(Box<OpenError>),
    /// A CSV body failed while it was read (transport, format, decode or limit).
    Read(Box<ReadError>),
    /// A job ended `Failed` or `Aborted`.
    Job(Box<JobFailure>),
    /// A CSV configuration or encoding failure before anything was sent.
    Csv(Box<crate::csv::CsvError>),
}

impl SfError {
    /// Converts a Concord error: a `Status` error's unread response is read (at most
    /// [`MAX_ERROR_BODY`] bytes) and decoded as a Salesforce error document; every other kind
    /// is kept as is.
    pub async fn from_concord(error: concord::Error) -> Self {
        if error.kind() != ErrorKind::Status {
            return SfError::Concord(error);
        }
        let operation = error.endpoint();
        match error.into_response() {
            Ok(response) => SfError::Api(Box::new(ApiFailure::read(operation, response).await)),
            Err(error) => SfError::Concord(error),
        }
    }

    /// Converts a CSV open refusal: a non-2xx status is read as an error document
    /// ([`SfError::Api`]); a refused 2xx head stays [`SfError::Open`].
    pub async fn from_open(error: OpenError) -> Self {
        if error.kind() != OpenErrorKind::Status {
            return SfError::Open(Box::new(error));
        }
        let operation = error.operation();
        let response = error.into_response();
        SfError::Api(Box::new(ApiFailure::read(operation, response).await))
    }

    /// The provider error document, for an [`SfError::Api`].
    pub fn api(&self) -> Option<&ApiFailure> {
        match self {
            SfError::Api(failure) => Some(failure),
            _ => None,
        }
    }

    /// Whether Salesforce reported this error code (`INVALID_QUERY_LOCATOR`, ...).
    pub fn has_error_code(&self, code: &str) -> bool {
        self.api().is_some_and(|a| a.has_error_code(code))
    }

    /// Whether a query locator was refused as invalid or expired (`INVALID_QUERY_LOCATOR`):
    /// the chain cannot resume from it and must restart.
    pub fn is_expired_locator(&self) -> bool {
        self.has_error_code("INVALID_QUERY_LOCATOR")
    }
}

impl From<ProtocolError> for SfError {
    fn from(e: ProtocolError) -> Self {
        SfError::Protocol(Box::new(e))
    }
}

impl From<JobFailure> for SfError {
    fn from(e: JobFailure) -> Self {
        SfError::Job(Box::new(e))
    }
}

impl From<concord::Error> for SfError {
    /// Keeps the error as is (a `Status` error keeps its unread response); use
    /// [`SfError::from_concord`] to read the provider's error document instead.
    fn from(e: concord::Error) -> Self {
        SfError::Concord(e)
    }
}

impl From<LinkError> for SfError {
    fn from(e: LinkError) -> Self {
        SfError::Link(e)
    }
}

impl From<ReadError> for SfError {
    fn from(e: ReadError) -> Self {
        SfError::Read(Box::new(e))
    }
}

impl From<crate::csv::CsvError> for SfError {
    fn from(e: crate::csv::CsvError) -> Self {
        SfError::Csv(Box::new(e))
    }
}

impl fmt::Display for SfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SfError::Concord(e) => fmt::Display::fmt(e, f),
            SfError::Api(e) => fmt::Display::fmt(e, f),
            SfError::Protocol(e) => fmt::Display::fmt(e, f),
            SfError::Link(e) => fmt::Display::fmt(e, f),
            SfError::Open(e) => fmt::Display::fmt(e, f),
            SfError::Read(e) => fmt::Display::fmt(e, f),
            SfError::Job(e) => fmt::Display::fmt(e, f),
            SfError::Csv(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl fmt::Debug for SfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SfError::Concord(e) => f.debug_tuple("Concord").field(e).finish(),
            SfError::Api(e) => f.debug_tuple("Api").field(e).finish(),
            SfError::Protocol(e) => f.debug_tuple("Protocol").field(e).finish(),
            SfError::Link(e) => f.debug_tuple("Link").field(e).finish(),
            SfError::Open(e) => f.debug_tuple("Open").field(e).finish(),
            SfError::Read(e) => f.debug_tuple("Read").field(e).finish(),
            SfError::Job(e) => f.debug_tuple("Job").field(e).finish(),
            SfError::Csv(e) => f.debug_tuple("Csv").field(e).finish(),
        }
    }
}

/// No `source()` chain: like `concord::Error`, the wrapped errors are reached through the enum
/// and their own deliberate accessors.
impl std::error::Error for SfError {}

/// What a non-2xx response said.
pub struct ApiFailure {
    operation: Option<&'static str>,
    status: StatusCode,
    errors: Vec<ApiErrorDoc>,
    body: ErrorBodyState,
}

/// Whether an error document was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorBodyState {
    /// Decoded into [`ApiFailure::errors`].
    Decoded,
    /// Empty body.
    Empty,
    /// Not a Salesforce error document.
    NotAnErrorDocument,
    /// Longer than [`MAX_ERROR_BODY`]: not decoded.
    TooLarge,
    /// The body could not be read (transport).
    Unreadable,
}

impl ApiFailure {
    async fn read(
        operation: Option<&'static str>,
        mut response: concord::reqwest::Response,
    ) -> Self {
        let status = response.status();
        let mut bytes = Vec::new();
        let body = loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if bytes.len() + chunk.len() > MAX_ERROR_BODY {
                        break ErrorBodyState::TooLarge;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(None) if bytes.is_empty() => break ErrorBodyState::Empty,
                Ok(None) => break ErrorBodyState::Decoded,
                Err(_) => break ErrorBodyState::Unreadable,
            }
        };
        let (errors, body) = if body == ErrorBodyState::Decoded {
            match <Json as Decode<ApiErrorBody>>::decode_body((), bytes.into()) {
                Ok(doc) => (doc.into_vec(), ErrorBodyState::Decoded),
                Err(_) => (Vec::new(), ErrorBodyState::NotAnErrorDocument),
            }
        } else {
            (Vec::new(), body)
        };
        Self {
            operation,
            status,
            errors,
            body,
        }
    }

    /// The operation (Concord endpoint name or SDK label).
    pub fn operation(&self) -> Option<&'static str> {
        self.operation
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The decoded error entries.
    pub fn errors(&self) -> &[ApiErrorDoc] {
        &self.errors
    }

    /// Whether the body was decoded.
    pub fn body_state(&self) -> ErrorBodyState {
        self.body
    }

    /// Whether an entry has this `errorCode`.
    pub fn has_error_code(&self, code: &str) -> bool {
        self.errors.iter().any(|e| e.error_code == code)
    }
}

impl fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(op) = self.operation {
            write!(f, "{op}: ")?;
        }
        write!(f, "Salesforce answered status {}", self.status.as_u16())?;
        if !self.errors.is_empty() {
            f.write_str(" [")?;
            for (i, e) in self.errors.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                // Error codes are upper-case identifiers chosen by Salesforce; anything else is
                // not echoed.
                if e.error_code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                {
                    f.write_str(&e.error_code)?;
                } else {
                    f.write_str("<unrecognized code>")?;
                }
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ApiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiFailure")
            .field("operation", &self.operation)
            .field("status", &self.status.as_u16())
            .field("errors", &self.errors)
            .field("body", &self.body)
            .finish()
    }
}

/// A response that contradicts the documented protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    operation: &'static str,
    kind: ProtocolErrorKind,
}

/// What contradicted the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProtocolErrorKind {
    /// `done` is false but no next link was returned: the chain cannot continue, and no locator
    /// is guessed (`totalSize` does not license one).
    MissingNextLink,
    /// `done` is true but a next link was returned.
    NextLinkWhenDone,
    /// Both `nextRecordsUrl` and `nextRecordUrl` were returned, with different values.
    AmbiguousNextLink,
    /// A job document's `columnDelimiter` or `lineEnding` differs from the dialect the job was
    /// created with.
    DialectChanged,
    /// A job document's `apiVersion` differs from the context's version.
    ApiVersionChanged,
    /// A job document names another job id.
    JobIdChanged,
    /// Results were requested for a job that is not `JobComplete`.
    JobNotComplete,
    /// A result page has no `Sforce-Locator` header.
    LocatorHeaderMissing,
    /// The `Sforce-Locator` header is not a valid locator.
    LocatorHeaderInvalid,
    /// `Sforce-NumberOfRecords` is missing or not a number.
    RecordCountHeaderInvalid,
    /// The parsed record count differs from `Sforce-NumberOfRecords`.
    RecordCountMismatch {
        /// The header's count.
        declared: u64,
        /// Records parsed.
        parsed: u64,
    },
    /// A result part's CSV header differs from the first part's.
    HeaderChanged,
    /// A record's `attributes.type` is not the queried object.
    UnexpectedRecordType,
    /// A record's `attributes.url` names another record.
    RecordLinkMismatch,
    /// A job stayed unfinished for every poll the caller allowed.
    PollBudgetExhausted,
    /// An ingest job is not `Open` when data is uploaded.
    JobNotOpen,
    /// An ingest result file's header lacks its `sf__` columns, or a value in them is malformed.
    ResultFileLayout,
}

impl ProtocolError {
    /// A protocol error of `operation`.
    pub fn new(operation: &'static str, kind: ProtocolErrorKind) -> Self {
        Self { operation, kind }
    }

    /// The operation.
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// What happened.
    pub fn kind(&self) -> &ProtocolErrorKind {
        &self.kind
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: protocol error: ", self.operation)?;
        match &self.kind {
            ProtocolErrorKind::MissingNextLink => f.write_str("not done, but no next link"),
            ProtocolErrorKind::NextLinkWhenDone => f.write_str("done, but a next link"),
            ProtocolErrorKind::AmbiguousNextLink => {
                f.write_str("nextRecordsUrl and nextRecordUrl disagree")
            }
            ProtocolErrorKind::DialectChanged => f.write_str("the job's CSV dialect changed"),
            ProtocolErrorKind::ApiVersionChanged => f.write_str("the job's API version differs"),
            ProtocolErrorKind::JobIdChanged => f.write_str("the document names another job"),
            ProtocolErrorKind::JobNotComplete => f.write_str("the job is not JobComplete"),
            ProtocolErrorKind::LocatorHeaderMissing => f.write_str("no Sforce-Locator header"),
            ProtocolErrorKind::LocatorHeaderInvalid => f.write_str("invalid Sforce-Locator"),
            ProtocolErrorKind::RecordCountHeaderInvalid => {
                f.write_str("missing or invalid Sforce-NumberOfRecords")
            }
            ProtocolErrorKind::RecordCountMismatch { declared, parsed } => write!(
                f,
                "Sforce-NumberOfRecords says {declared}, {parsed} records were parsed"
            ),
            ProtocolErrorKind::HeaderChanged => {
                f.write_str("a result part's header differs from the first part's")
            }
            ProtocolErrorKind::UnexpectedRecordType => {
                f.write_str("a record of another object type")
            }
            ProtocolErrorKind::RecordLinkMismatch => {
                f.write_str("a record's attributes.url names another record")
            }
            ProtocolErrorKind::PollBudgetExhausted => {
                f.write_str("the job did not finish within the poll budget")
            }
            ProtocolErrorKind::JobNotOpen => f.write_str("the ingest job is not Open"),
            ProtocolErrorKind::ResultFileLayout => {
                f.write_str("unexpected ingest result file layout")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Which returned link was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkRole {
    /// A REST query's `nextRecordsUrl`.
    QueryNext,
    /// A `resultUrl` of a result-pages manifest.
    ResultUrl,
    /// The next result-pages manifest link.
    ResultPagesNext,
    /// An ingest job's `contentUrl`.
    ContentUrl,
    /// A record's `attributes.url`.
    RecordUrl,
}

/// Why a returned link was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkErrorKind {
    /// Not a URL or URL path, or it contains whitespace, control characters or backslashes.
    Malformed,
    /// Another scheme, host or port than the org's instance (including `//host/...`).
    ForeignOrigin,
    /// User information in the URL.
    UserInfo,
    /// A fragment.
    Fragment,
    /// Dot segments or characters the URL parser would rewrite.
    NonCanonicalPath,
    /// Under `/services/data/` with another API version.
    VersionMismatch,
    /// Not the resource this link must designate.
    UnexpectedPath,
    /// Another job's resource.
    WrongJob,
    /// Unexpected, repeated or malformed query parameters.
    UnexpectedQuery,
    /// A locator that is not a valid locator.
    InvalidLocator,
    /// A `maxRecords` that is not a positive number.
    InvalidMaxRecords,
}

/// A returned link refused before use. Never shows the link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkError {
    role: LinkRole,
    kind: LinkErrorKind,
}

impl LinkError {
    pub(crate) fn new(role: LinkRole, kind: LinkErrorKind) -> Self {
        Self { role, kind }
    }

    /// Which link.
    pub fn role(&self) -> LinkRole {
        self.role
    }

    /// Why.
    pub fn kind(&self) -> LinkErrorKind {
        self.kind
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let role = match self.role {
            LinkRole::QueryNext => "nextRecordsUrl",
            LinkRole::ResultUrl => "resultUrl",
            LinkRole::ResultPagesNext => "result-pages next link",
            LinkRole::ContentUrl => "contentUrl",
            LinkRole::RecordUrl => "attributes.url",
        };
        let kind = match self.kind {
            LinkErrorKind::Malformed => "malformed",
            LinkErrorKind::ForeignOrigin => "not on the org's instance origin",
            LinkErrorKind::UserInfo => "contains user information",
            LinkErrorKind::Fragment => "contains a fragment",
            LinkErrorKind::NonCanonicalPath => "non-canonical path",
            LinkErrorKind::VersionMismatch => "another API version",
            LinkErrorKind::UnexpectedPath => "unexpected resource",
            LinkErrorKind::WrongJob => "another job's resource",
            LinkErrorKind::UnexpectedQuery => "unexpected query parameters",
            LinkErrorKind::InvalidLocator => "invalid locator",
            LinkErrorKind::InvalidMaxRecords => "invalid maxRecords",
        };
        write!(f, "returned {role} refused: {kind}")
    }
}

impl std::error::Error for LinkError {}

/// A job that ended `Failed` or `Aborted`. Its `errorMessage` is withheld from `Display`/`Debug`.
#[derive(Clone)]
pub struct JobFailure {
    job: String,
    state: JobState,
    error_message: Option<String>,
}

impl JobFailure {
    pub(crate) fn new(
        job: impl Into<String>,
        state: JobState,
        error_message: Option<String>,
    ) -> Self {
        Self {
            job: job.into(),
            state,
            error_message,
        }
    }

    /// The job id.
    pub fn job(&self) -> &str {
        &self.job
    }

    /// The final state.
    pub fn state(&self) -> &JobState {
        &self.state
    }

    /// The job's `errorMessage`, **unredacted**.
    pub fn error_message_unredacted(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

impl fmt::Display for JobFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bulk job {} ended {}", self.job, self.state.as_str())
    }
}

impl fmt::Debug for JobFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobFailure")
            .field("job", &self.job)
            .field("state", &self.state)
            .field(
                "error_message",
                &self.error_message.as_ref().map(|_| "<withheld>"),
            )
            .finish()
    }
}
