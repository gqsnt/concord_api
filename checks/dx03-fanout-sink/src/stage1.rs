//! Collect a finished Bulk query's sequential CSV result chain into memory.

use std::{error::Error, fmt, num::NonZeroUsize};

use concord_example_salesforce_sdk::{
    OrgContext, ProtocolError, ProtocolErrorKind, SfError,
    bulk::{BulkDialect, JobState},
    bulk_query::{BulkQueryJob, ResultSets},
    ids::{MaxRecords, QueryJobId, RecordId},
    soql::FieldPath,
};
use concord_pagination::{Budget, Outcome, Resume, sequential};
use serde::Deserialize;

/// The required query projection, in wire-header order.
pub const ACCOUNT_COLUMNS: [&str; 3] = ["Id", "Name", "NumberOfEmployees"];

/// An Account selected with [`ACCOUNT_COLUMNS`].
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct AccountRow {
    /// Salesforce record identifier.
    #[serde(rename = "Id")]
    pub id: RecordId,
    /// Account name, preserving whitespace, quotes and embedded line endings.
    #[serde(rename = "Name")]
    pub name: String,
    /// Empty quoted or unquoted fields both become `None`.
    #[serde(rename = "NumberOfEmployees")]
    pub number_of_employees: Option<u64>,
}

/// A complete export in provider order.
#[derive(Debug)]
pub struct AccountExport {
    /// All rows from all result sets, collected in memory.
    pub rows: Vec<AccountRow>,
    /// Result-set requests, excluding the initial job-info request.
    pub result_requests: u64,
}

/// A failed export; partial rows are discarded, never returned as a complete export.
#[derive(Debug)]
pub enum ExportError {
    /// The SDK's original typed API, protocol, transport or CSV failure.
    Salesforce(SfError),
    /// Job information omitted the delimiter or line ending needed to attach the job.
    MissingDialect,
    /// Traversal stopped without observing the provider's last result set.
    Incomplete(Outcome),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Salesforce(error) => write!(f, "Account export: {error}"),
            Self::MissingDialect => {
                f.write_str("Account export: job information omitted its CSV dialect")
            }
            Self::Incomplete(outcome) => {
                write!(f, "Account export: incomplete traversal ({outcome:?})")
            }
        }
    }
}

impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Salesforce(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SfError> for ExportError {
    fn from(error: SfError) -> Self {
        Self::Salesforce(error)
    }
}

fn invalid_job(kind: ProtocolErrorKind) -> ExportError {
    SfError::Protocol(Box::new(ProtocolError::new(
        "stage1.collect_accounts",
        kind,
    )))
    .into()
}

/// Download every result set of an already completed query selecting [`ACCOUNT_COLUMNS`].
///
/// Fetches job information once to obtain its dialect, then follows the sequential
/// `Sforce-Locator` chain. Each result body is fully read before requesting the next.
/// Requests at most 10,000 rows per result set and accumulates every set in a `Vec`;
/// total memory therefore grows with the export. Nothing is retried.
///
/// The pagination budget is `usize::MAX` factory calls (no practical application cap).
/// Success requires provider exhaustion, even if that budget were reached.
///
/// # Errors
///
/// [`ExportError`] preserves SDK failures, rejects mismatched job ids/API versions,
/// unfinished jobs and absent dialect metadata, and never returns a truncated success.
pub async fn collect_accounts(
    ctx: &OrgContext,
    job_id: QueryJobId,
) -> Result<AccountExport, ExportError> {
    let info = match ctx
        .api()
        .bulk_query()
        .job(job_id.clone())
        .info()
        .send()
        .await
    {
        Ok(info) => info,
        Err(error) => return Err(SfError::from_concord(error).await.into()),
    };
    if info.id != job_id {
        return Err(invalid_job(ProtocolErrorKind::JobIdChanged));
    }
    if info.api_version != ctx.version() {
        return Err(invalid_job(ProtocolErrorKind::ApiVersionChanged));
    }
    if info.state != JobState::JobComplete {
        return Err(invalid_job(ProtocolErrorKind::JobNotComplete));
    }
    let dialect = BulkDialect::new(
        info.column_delimiter.ok_or(ExportError::MissingDialect)?,
        info.line_ending.ok_or(ExportError::MissingDialect)?,
    );
    let job = BulkQueryJob::attach(ctx, job_id, dialect);
    let columns =
        ACCOUNT_COLUMNS.map(|name| FieldPath::parse(name).expect("static API field name"));
    let sets = ResultSets::<AccountRow>::new(
        &job,
        &columns,
        Some(MaxRecords::new(10_000).expect("positive constant")),
    )?;
    let mut pages = sequential(Resume::Start, Budget::new(NonZeroUsize::MAX), |key| {
        sets.fetch(key)
    });
    let mut rows = Vec::new();
    while let Some(page) = pages.next().await {
        rows.extend(page?.items);
    }
    let report = pages.finish();
    if report.outcome != Outcome::Exhausted {
        return Err(ExportError::Incomplete(report.outcome));
    }
    Ok(AccountExport {
        rows,
        result_requests: report.progress.admitted,
    })
}
