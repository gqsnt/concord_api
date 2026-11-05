//! SF-B ingest sub-scenario: a constrained, offline-by-default Bulk API 2.0 insert of Contacts.
//!
//! Required encoder usage, not an ingestion product: the rows (a one-shot stream) are encoded by
//! the public CSV encoder in the job's dialect, with the Salesforce header and value rules
//! ([`SalesforceCsv`], [`FieldColumns`], [`IngestValue`]), into a **bounded spool** of known
//! length; then the job is created, the body uploaded in one `Content-Length` request, the upload
//! closed, the job polled, and the three result files read.
//!
//! Why a spool: Salesforce documents a per-upload size limit (100 MB of CSV, 150 MB as base64)
//! and no support for chunked uploads; the SDK does not rely on undocumented chunked acceptance.
//! The cost is reported: [`UploadReceipt::buffered_bytes`] bytes were held in memory before the
//! request started. This is not zero-prebuffer streaming.
//!
//! Every step after spooling **writes to the org**; live use needs an explicit sandbox mutation
//! opt-in (see the `sf_b` binary).

use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::num::NonZeroUsize;

use concord_example_salesforce_sdk::bulk::{
    BulkDialect, CreateIngestJob, IngestOperation, JobState,
};
use concord_example_salesforce_sdk::csv::{
    CsvError, FieldColumns, FieldText, IngestValue, SalesforceCsv,
};
use concord_example_salesforce_sdk::ids::{IngestJobId, SObjectName};
use concord_example_salesforce_sdk::ingest::{
    IngestJob, IngestResultRow, IngestWait, MAX_UPLOAD_BYTES, ResultFile,
};
use concord_example_salesforce_sdk::{OrgContext, SfError};
use futures_core::Stream;
use serde::Serialize;

/// One Contact to insert. Field order is the header order ([`CONTACT_INSERT_COLUMNS`]).
#[derive(Clone, Debug, Serialize)]
pub struct ContactInsert {
    /// `LastName` (required by Salesforce for a Contact).
    #[serde(rename = "LastName")]
    pub last_name: FieldText,
    /// `FirstName`.
    #[serde(rename = "FirstName")]
    pub first_name: IngestValue,
    /// `Email`.
    #[serde(rename = "Email")]
    pub email: IngestValue,
    /// `Loyalty_Tier__c` (custom).
    #[serde(rename = "Loyalty_Tier__c")]
    pub loyalty_tier: IngestValue,
    /// `Description` (may hold delimiters, quotes and line breaks).
    #[serde(rename = "Description")]
    pub description: IngestValue,
}

/// The header of [`ContactInsert`].
pub const CONTACT_INSERT_COLUMNS: [&str; 5] = [
    "LastName",
    "FirstName",
    "Email",
    "Loyalty_Tier__c",
    "Description",
];

/// The sub-scenario's settings.
#[derive(Clone, Debug)]
pub struct IngestInput {
    /// The job's dialect (sent explicitly; the upload uses it).
    pub dialect: BulkDialect,
    /// The most bytes spooled (at most [`MAX_UPLOAD_BYTES`]).
    pub upload_limit: usize,
    /// The most status polls.
    pub max_polls: NonZeroUsize,
    /// The most rows read from each result file (the rest is left unread and reported).
    pub max_result_rows: usize,
}

/// What was uploaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadReceipt {
    /// Data rows.
    pub rows: u64,
    /// Body bytes, all buffered before the request started (the spool's cost).
    pub buffered_bytes: usize,
}

/// One result file's rows.
#[derive(Debug, Default)]
pub struct ResultRows {
    /// The rows read.
    pub rows: Vec<IngestResultRow>,
    /// Whether rows remained unread past `max_result_rows`.
    pub truncated: bool,
}

/// The sub-scenario's result.
#[derive(Debug)]
pub struct IngestReport {
    /// The job.
    pub job: IngestJobId,
    /// Its dialect.
    pub dialect: BulkDialect,
    /// The upload.
    pub upload: UploadReceipt,
    /// The final state (`JobComplete`, `Failed`, or an unfinished state after the poll budget).
    pub state: JobState,
    /// `numberRecordsProcessed`.
    pub processed: Option<u64>,
    /// `numberRecordsFailed`.
    pub failed_count: Option<u64>,
    /// `successfulResults/` (read once the job finished).
    pub successful: ResultRows,
    /// `failedResults/`.
    pub failed: ResultRows,
    /// `unprocessedrecords/`.
    pub unprocessed: ResultRows,
}

/// A failure of the sub-scenario.
#[derive(Debug)]
#[non_exhaustive]
pub enum IngestError {
    /// Encoding into the spool failed (or exceeded the limit); no job was created.
    Encode(CsvError),
    /// Creating the job failed.
    Create(SfError),
    /// A step after creation failed; the job exists and may still be `Open`.
    Job {
        /// The job.
        job: IngestJobId,
        /// The failing step (`"upload"`, `"close"`, `"poll"`, `"results"`).
        step: &'static str,
        /// The error.
        error: SfError,
    },
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestError::Encode(e) => write!(f, "ingest encoding: {e}"),
            IngestError::Create(e) => write!(f, "ingest job creation: {e}"),
            IngestError::Job { job, step, error } => {
                write!(f, "ingest job {job}: {step} failed: {error}")
            }
        }
    }
}

impl StdError for IngestError {}

/// Runs the ingest sub-scenario with `rows` (a one-shot stream, polled once).
///
/// # Errors
///
/// [`IngestError`].
pub async fn ingest_round_trip<S, E, W, F>(
    ctx: &OrgContext,
    input: &IngestInput,
    rows: S,
    wait: W,
) -> Result<IngestReport, IngestError>
where
    S: Stream<Item = Result<ContactInsert, E>>,
    E: Into<Box<dyn StdError + Send + Sync>>,
    W: FnMut(u32) -> F,
    F: Future<Output = ()>,
{
    // Encode first: an encoding failure creates no job.
    let csv = SalesforceCsv::new(input.dialect);
    let columns = FieldColumns::new(CONTACT_INSERT_COLUMNS).map_err(IngestError::Encode)?;
    let limit = input.upload_limit.min(MAX_UPLOAD_BYTES);
    let spooled = csv
        .spool_stream(&columns, rows, limit)
        .await
        .map_err(IngestError::Encode)?;
    let upload = UploadReceipt {
        rows: spooled.rows(),
        buffered_bytes: spooled.len(),
    };

    let object = SObjectName::new("Contact").expect("a valid object name");
    let body = CreateIngestJob::new(object, IngestOperation::Insert, input.dialect);
    let (job, _) = IngestJob::create(ctx, body)
        .await
        .map_err(IngestError::Create)?;
    let step = |step: &'static str| {
        let id = job.id().clone();
        move |error| IngestError::Job {
            job: id,
            step,
            error,
        }
    };
    job.upload_spooled(spooled).await.map_err(step("upload"))?;
    job.close().await.map_err(step("close"))?;
    let info = match job
        .wait_finished(input.max_polls, wait)
        .await
        .map_err(step("poll"))?
    {
        IngestWait::Complete(info) | IngestWait::Failed(info) => info,
        IngestWait::StillRunning(info) => {
            return Ok(IngestReport {
                job: job.id().clone(),
                dialect: input.dialect,
                upload,
                state: info.state.clone(),
                processed: info.number_records_processed,
                failed_count: info.number_records_failed,
                successful: ResultRows::default(),
                failed: ResultRows::default(),
                unprocessed: ResultRows::default(),
            });
        }
    };
    let successful = read_rows(&job, ResultFile::Successful, input.max_result_rows)
        .await
        .map_err(step("results"))?;
    let failed = read_rows(&job, ResultFile::Failed, input.max_result_rows)
        .await
        .map_err(step("results"))?;
    let unprocessed = read_rows(&job, ResultFile::Unprocessed, input.max_result_rows)
        .await
        .map_err(step("results"))?;
    Ok(IngestReport {
        job: job.id().clone(),
        dialect: input.dialect,
        upload,
        state: info.state.clone(),
        processed: info.number_records_processed,
        failed_count: info.number_records_failed,
        successful,
        failed,
        unprocessed,
    })
}

async fn read_rows(job: &IngestJob, file: ResultFile, max: usize) -> Result<ResultRows, SfError> {
    let mut rows = job.result_rows(file).await?;
    let mut out = ResultRows::default();
    while let Some(row) = rows.next().await {
        if out.rows.len() == max {
            out.truncated = true;
            break;
        }
        out.rows.push(row?);
    }
    Ok(out)
}
