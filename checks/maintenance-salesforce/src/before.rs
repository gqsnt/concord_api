//! The frozen initial consumer (stage 1 of the maintenance exercise).
//!
//! A downstream **mailing export**: it exports the SDK's Contact export
//! ([`CONTACT_EXPORT_FIELDS`], read as [`ContactExportRow`]) through a Bulk API 2.0 query job,
//! in the CSV dialect the job is created with — the deployment's is comma / LF
//! ([`MailingExport::new`]) — and summarizes the typed rows.
//!
//! 1. **Describe.** `GET sobjects/Contact/describe/`; every export column must exist in the org
//!    (the custom `Loyalty_Tier__c` may not). A missing column refuses the export before any job
//!    is created ([`MailingError::MissingFields`]).
//! 2. **Statement.** The select list is the export's columns, in order; the caller's filter is a
//!    SOQL literal.
//! 3. **Job.** Created with the dialect stated explicitly, polled with the caller's timer.
//! 4. **Results.** The `Sforce-Locator` chain of result sets through the SDK's [`ResultSets`]
//!    factory and `concord_pagination::sequential`, under a result-set budget; each set is parsed
//!    with the job's dialect.
//!
//! Written only against the SDK's public exports, `concord` and `concord-pagination`. Never
//! edited after stage 1.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::num::NonZeroUsize;

use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::bulk_query::{BulkQueryJob, JobWait, ResultSets};
use concord_example_salesforce_sdk::csv::{CONTACT_EXPORT_FIELDS, ContactExportRow};
use concord_example_salesforce_sdk::ids::{
    FieldName, InvalidValue, MaxRecords, QueryJobId, RecordId, ResultLocator, SObjectName,
};
use concord_example_salesforce_sdk::soql::{Condition, FieldPath, Literal, Op, Soql};
use concord_example_salesforce_sdk::{OrgContext, SfError};
use concord_pagination::{Budget, Outcome, Resume, sequential};

/// What to export.
#[derive(Clone, Debug)]
pub struct MailingExport {
    /// Only Contacts whose e-mail ends with this text (`Email LIKE '%..'`), when set.
    pub email_suffix: Option<String>,
    /// `LIMIT` of the statement, when set.
    pub limit: Option<u32>,
    /// The job's CSV dialect, sent explicitly at creation.
    pub dialect: BulkDialect,
    /// `maxRecords` of each result set, when set.
    pub max_records: Option<MaxRecords>,
    /// At most this many result sets are read.
    pub set_budget: Budget,
    /// At most this many job status polls.
    pub max_polls: NonZeroUsize,
}

impl MailingExport {
    /// The deployment's export: comma / LF (stated explicitly), no filter, no limit.
    pub fn new(set_budget: Budget, max_polls: NonZeroUsize) -> Self {
        Self {
            email_suffix: None,
            limit: None,
            dialect: BulkDialect::new(ColumnDelimiter::Comma, LineEnding::Lf),
            max_records: None,
            set_budget,
            max_polls,
        }
    }
}

/// One export column as the org describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescribedColumn {
    /// The API name.
    pub name: String,
    /// The described type (`id`, `string`, `email`, `reference`, `picklist`, ...).
    pub field_type: String,
    /// Whether it is a custom field.
    pub custom: bool,
}

/// What an export returned.
#[derive(Debug)]
pub struct ExportedContacts {
    /// The query job.
    pub job: QueryJobId,
    /// The dialect the job was created (and its results read) with.
    pub dialect: BulkDialect,
    /// The export columns, as described by the org, in select-list order.
    pub columns: Vec<DescribedColumn>,
    /// The rows, in result order.
    pub rows: Vec<ContactExportRow>,
    /// Result sets read.
    pub result_sets: u64,
    /// CSV bytes the readers consumed.
    pub bytes_read: u64,
    /// `Exhausted`, or `Bounded` by the result-set budget.
    pub outcome: Outcome,
    /// Where to continue when bounded.
    pub resume: Option<Resume<ResultLocator>>,
}

impl ExportedContacts {
    /// Contacts per loyalty tier (`None`: no tier).
    pub fn by_loyalty_tier(&self) -> BTreeMap<Option<String>, usize> {
        let mut tiers = BTreeMap::new();
        for row in &self.rows {
            *tiers.entry(row.loyalty_tier.clone()).or_insert(0) += 1;
        }
        tiers
    }

    /// Contacts that cannot be mailed: no e-mail address.
    pub fn without_email(&self) -> Vec<&RecordId> {
        self.rows
            .iter()
            .filter(|row| row.email.is_none())
            .map(|row| &row.id)
            .collect()
    }
}

/// Why an export did not run to its end.
#[derive(Debug)]
#[non_exhaustive]
pub enum MailingError {
    /// An export column or the filter could not form a statement.
    Query(InvalidValue),
    /// The describe call failed.
    Describe(SfError),
    /// The org lacks these export columns; no job was created.
    MissingFields(Vec<String>),
    /// Creating or polling the job failed, or the job ended `Failed`/`Aborted`.
    Job(SfError),
    /// The job was still running after every allowed poll.
    StillRunning(QueryJobId),
    /// Reading a result set failed (request, head, CSV body or a protocol check).
    Results(SfError),
}

impl fmt::Display for MailingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MailingError::Query(e) => write!(f, "export statement: {e}"),
            MailingError::Describe(e) => write!(f, "describe Contact: {e}"),
            MailingError::MissingFields(fields) => {
                write!(f, "the org lacks export columns: {}", fields.join(", "))
            }
            MailingError::Job(e) => write!(f, "export job: {e}"),
            MailingError::StillRunning(job) => write!(f, "export job {job} still running"),
            MailingError::Results(e) => write!(f, "export results: {e}"),
        }
    }
}

impl std::error::Error for MailingError {}

/// The export statement: the export columns, the e-mail filter, the limit.
///
/// # Errors
///
/// [`MailingError::Query`].
pub fn export_soql(export: &MailingExport) -> Result<Soql, MailingError> {
    let fields = CONTACT_EXPORT_FIELDS
        .iter()
        .map(|name| FieldPath::parse(name))
        .collect::<Result<Vec<_>, _>>()
        .map_err(MailingError::Query)?;
    let contact = SObjectName::new("Contact").map_err(MailingError::Query)?;
    let mut statement = Soql::select(contact, fields);
    if let Some(suffix) = &export.email_suffix {
        let email = FieldName::new("Email").map_err(MailingError::Query)?;
        statement = statement.filter(Condition::new(
            email,
            Op::Like,
            Literal::like_suffix(suffix),
        ));
    }
    if let Some(limit) = export.limit {
        statement = statement.limit(limit);
    }
    statement.build().map_err(MailingError::Query)
}

/// Runs the export (**creates a Bulk query job in the org**), waiting `wait(poll)` between job
/// polls.
///
/// # Errors
///
/// [`MailingError`].
pub async fn export_contacts<W, F>(
    ctx: &OrgContext,
    export: &MailingExport,
    wait: W,
) -> Result<ExportedContacts, MailingError>
where
    W: FnMut(u32) -> F,
    F: Future<Output = ()>,
{
    // 1. Describe: every export column must exist in this org.
    let describe = match ctx.api().contacts().describe().send().await {
        Ok(describe) => describe,
        Err(e) => return Err(MailingError::Describe(SfError::from_concord(e).await)),
    };
    let mut columns = Vec::new();
    let mut missing = Vec::new();
    for name in CONTACT_EXPORT_FIELDS {
        let field = FieldName::new(name).map_err(MailingError::Query)?;
        match describe.field(&field) {
            Some(described) => columns.push(DescribedColumn {
                name: described.name.to_string(),
                field_type: described.field_type.clone(),
                custom: described.custom,
            }),
            None => missing.push(name.to_owned()),
        }
    }
    if !missing.is_empty() {
        return Err(MailingError::MissingFields(missing));
    }

    // 2.-3. The job, in the export's dialect.
    let soql = export_soql(export)?;
    let (job, _) = BulkQueryJob::create(ctx, &soql, export.dialect)
        .await
        .map_err(MailingError::Job)?;
    match job
        .wait_complete(export.max_polls, wait)
        .await
        .map_err(MailingError::Job)?
    {
        JobWait::Complete(_) => {}
        JobWait::StillRunning(_) => return Err(MailingError::StillRunning(job.id().clone())),
    }

    // 4. The result sets, typed, with the job's dialect.
    let fields = CONTACT_EXPORT_FIELDS
        .iter()
        .map(|name| FieldPath::parse(name))
        .collect::<Result<Vec<_>, _>>()
        .map_err(MailingError::Query)?;
    let sets = ResultSets::<ContactExportRow>::new(&job, &fields, export.max_records)
        .map_err(MailingError::Results)?;
    let mut chain = sequential(Resume::Start, export.set_budget, |key| sets.fetch(key));
    let mut rows = Vec::new();
    let mut result_sets = 0;
    let mut bytes_read = 0;
    while let Some(set) = chain.next().await {
        let set = set.map_err(MailingError::Results)?;
        result_sets += 1;
        bytes_read += set.meta.bytes;
        rows.extend(set.items);
    }
    let report = chain.finish();
    Ok(ExportedContacts {
        job: job.id().clone(),
        dialect: job.dialect(),
        columns,
        rows,
        result_sets,
        bytes_read,
        outcome: report.outcome,
        resume: report.resume,
    })
}
