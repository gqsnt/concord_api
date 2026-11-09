//! SF-B — streamed Bulk CSV round trip (export side).
//!
//! Input: a bounded Contact export selection, an explicit Bulk CSV dialect, and concurrency /
//! discovery / backlog budgets. Output: an [`ExportReport`] — the job, every discovered result
//! part identified as completed, completed-but-unemitted (retained), failed, cancelled or queued,
//! the discovery state, the combined output when everything succeeded, and an
//! [`ExportRestart`] otherwise.
//!
//! # Path
//!
//! 1. Create the query job with the dialect stated explicitly; poll it (the caller's timer)
//!    until `JobComplete`. Every job document is checked against the context's API version and
//!    the creation dialect ([`BulkQueryJob`]).
//! 2. Discover result parts with `concord_pagination::manifest_jobs`: the SDK's
//!    [`ResultManifests`] factory reads one result-pages manifest per call (at most five result
//!    URLs), validates every link against the org, version and job, and numbers the parts.
//!    Discovery calls, queued descriptors and running downloads each have an explicit bound.
//! 3. Each part is one job future ([`PartRunner::run`]): the SDK's
//!    [`ResultPartReader`] requests the result URL and opens the body with the job's dialect (its
//!    own header, which must be exactly the selected fields); the rows are parsed incrementally
//!    and written in batches to a staged shard file; the shard is published only after the SDK's
//!    validated end of the part ([`ResultPartRows::finish`]: end of input reached, row count equal
//!    to `Sforce-NumberOfRecords` when declared). The window slot is held through all of it: a
//!    response whose headers arrived but whose body stalls still occupies its slot.
//! 4. When every part succeeded and discovery ended, combine the header and the shards in
//!    discovery order into the output file (in the job's dialect). Headers of the parts are never
//!    concatenated: each part was parsed with its own header.
//!
//! # Failures, replay and restart
//!
//! - A part that fails after rows were parsed (a transport error mid-body, a width or schema
//!   error) removes its staged shard: nothing of it is published, so the combined output can
//!   neither lose nor duplicate rows. [`FailedPart::rows_discarded`] says how many parsed rows
//!   were thrown away and will be downloaded again: a re-run re-requests the part's own locator
//!   ("as long as the associated job exists, the locator string for a set of results does not
//!   change").
//! - With [`OnError::Stop`] (the default) the first observed failure stops admission and drops
//!   the other running downloads (cancelled; their partial shards are removed). With
//!   [`OnError::Continue`], every admitted and queued part runs.
//! - Parts that completed while an earlier part was still running are published shards even if
//!   the stop prevented their ordered emission: they are reported as `retained` and reused.
//! - A failed manifest call ends discovery: work after it is unknown ([`DiscoveryState`]).
//! - [`ExportRestart`] keeps the published receipts, the parts to run again (failed, cancelled,
//!   queued) and the discovery resume point; [`continue_export`] runs them (the re-runs through
//!   `concord_pagination::independent`, the rest of discovery through `manifest_jobs` again) and
//!   combines when everything is done. A receipt whose file is missing or changed is run again.
//!
//! Exactly-once delivery to an arbitrary sink is not claimed: the guarantee is about the shard
//! files this module owns.

use std::fmt;
use std::future::Future;
use std::io;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use concord::csv::{CsvWriter, Rows};
use concord_example_salesforce_sdk::bulk::BulkDialect;
use concord_example_salesforce_sdk::bulk_query::{
    BulkQueryJob, JobWait, ManifestCursor, ResultManifests, ResultPart, ResultPartReader,
    ResultPartRows,
};
use concord_example_salesforce_sdk::csv::{
    CONTACT_EXPORT_FIELDS, ContactExportRow, CsvError, FieldColumns, SalesforceCsv,
};
use concord_example_salesforce_sdk::ids::{ApiVersion, FieldName, QueryJobId, SObjectName};
use concord_example_salesforce_sdk::soql::{Condition, FieldPath, Literal, Op, Soql};
use concord_example_salesforce_sdk::{OrgContext, SfError};
use concord_pagination::{
    Budget, JobOptions, JobsError, OnError, Options, Order, Outcome, Resume, Unfinished,
    independent, manifest_jobs,
};

use crate::sink::{CombinedOutput, ShardReceipt, ShardStaging, combine};

/// Rows encoded per shard write.
const SHARD_BATCH: usize = 256;

/// The workflow's input.
#[derive(Clone, Debug)]
pub struct ExportInput {
    /// Only Contacts whose e-mail ends with this text, when set.
    pub email_suffix: Option<String>,
    /// `LIMIT` of the export query, when set (a bounded export).
    pub limit: Option<u32>,
    /// The job's CSV dialect, sent explicitly at creation.
    pub dialect: BulkDialect,
    /// Concurrent part downloads (running plus completed-and-retained for ordered emission).
    pub window: NonZeroUsize,
    /// The most part downloads per traversal (re-runs and discovery each have this budget).
    pub job_budget: Budget,
    /// The most manifest calls (empty manifests included).
    pub discovery_budget: Budget,
    /// The most discovered parts waiting for a download slot.
    pub backlog: NonZeroUsize,
    /// Emission order of completed parts.
    pub order: Order,
    /// Failure policy.
    pub on_error: OnError,
    /// The most job status polls before giving up for now.
    pub max_polls: NonZeroUsize,
    /// The most CSV bytes read from one part (`None`: no total limit; the per-record limits of
    /// the codec still apply).
    pub max_part_bytes: Option<u64>,
    /// The staging directory for shards (caller-owned).
    pub staging: PathBuf,
    /// The combined output file.
    pub output: PathBuf,
}

/// The export query of an input: the Contact export columns, filtered and limited.
///
/// # Errors
///
/// Never for the fixed columns; a statement error otherwise.
pub fn export_soql(input: &ExportInput) -> Result<Soql, ExportError> {
    let fields = CONTACT_EXPORT_FIELDS
        .iter()
        .map(|f| FieldPath::parse(f))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ExportError::Query)?;
    let object = SObjectName::new("Contact").map_err(ExportError::Query)?;
    let mut builder = Soql::select(object, fields);
    if let Some(suffix) = &input.email_suffix {
        let email = FieldName::new("Email").map_err(ExportError::Query)?;
        builder = builder.filter(Condition::new(
            email,
            Op::Like,
            Literal::like_suffix(suffix),
        ));
    }
    if let Some(limit) = input.limit {
        builder = builder.limit(limit);
    }
    builder.build().map_err(ExportError::Query)
}

/// A failure that prevents the export from running.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_export_error)]
pub enum ExportError {
    /// The statement could not be built.
    #[from(skip)]
    Query(concord_example_salesforce_sdk::ids::InvalidValue),
    /// Creating or polling the job failed, or the job ended `Failed`/`Aborted`.
    #[from(skip)]
    Job(SfError),
    /// The staging directory could not be used.
    #[from(skip)]
    Staging(io::Error),
    /// The CSV dialect or header was refused.
    #[from(skip)]
    Csv(CsvError),
    /// Combining the shards failed (they stay published; the restart still holds them).
    #[from(skip)]
    Combine(io::Error),
}

fn display_export_error(value: &ExportError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        ExportError::Query(e) => write!(f, "export query: {e}"),
        ExportError::Job(e) => write!(f, "export job: {e}"),
        ExportError::Staging(e) => write!(f, "staging directory: {}", e.kind()),
        ExportError::Csv(e) => write!(f, "export CSV: {e}"),
        ExportError::Combine(e) => write!(f, "combining shards: {}", e.kind()),
    }
}

/// Why one part failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum PartError {
    /// The request, the head, the body or a protocol check.
    Sf(SfError),
    /// Writing the staged shard.
    Sink(io::Error),
    /// Encoding rows for the shard.
    Encode(Box<CsvError>),
}

impl fmt::Display for PartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartError::Sf(e) => fmt::Display::fmt(e, f),
            PartError::Sink(e) => write!(f, "shard write failed: {}", e.kind()),
            PartError::Encode(e) => fmt::Display::fmt(e, f),
        }
    }
}

/// A part job's failure: the error and the parsed rows that were discarded with the staged
/// shard (they will be downloaded again by a re-run).
#[derive(Debug)]
pub struct PartFailure {
    /// The error.
    pub error: PartError,
    /// Rows parsed (and staged) before the failure, discarded.
    pub rows_discarded: u64,
}

impl fmt::Display for PartFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} parsed rows discarded)",
            self.error, self.rows_discarded
        )
    }
}

impl std::error::Error for PartFailure {}

/// One part job: download, parse with the job's dialect, stage, publish. Cheap to clone.
#[derive(Clone, Debug)]
pub struct PartRunner {
    parts: ResultPartReader<ContactExportRow>,
    writer: CsvWriter,
    staging: ShardStaging,
    header: Vec<u8>,
}

impl PartRunner {
    /// A runner for `job`'s Contact export parts, staging into `staging`.
    ///
    /// # Errors
    ///
    /// [`ExportError::Csv`] (or [`ExportError::Query`] for the fixed columns, never expected).
    pub fn new(
        job: &BulkQueryJob,
        staging: ShardStaging,
        max_part_bytes: Option<u64>,
    ) -> Result<Self, ExportError> {
        let csv = SalesforceCsv::new(job.dialect());
        let columns = FieldColumns::new(CONTACT_EXPORT_FIELDS).map_err(ExportError::Csv)?;
        let fields = CONTACT_EXPORT_FIELDS
            .iter()
            .map(|f| FieldPath::parse(f))
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExportError::Query)?;
        let mut parts = ResultPartReader::new(job, &fields).map_err(ExportError::Csv)?;
        if let Some(max) = max_part_bytes {
            parts = parts.max_part_bytes(max).map_err(ExportError::Csv)?;
        }
        let writer = csv.rows_writer(&columns).map_err(ExportError::Csv)?;
        let header = csv.header(&columns).map_err(ExportError::Csv)?;
        Ok(Self {
            parts,
            writer,
            staging,
            header,
        })
    }

    /// The output header (the job's dialect).
    pub fn header(&self) -> &[u8] {
        &self.header
    }

    /// The staging directory.
    pub fn staging(&self) -> &ShardStaging {
        &self.staging
    }

    /// The whole job of one part, as one owned future: request acquisition, incremental parsing
    /// through EOF, shard writes, the SDK's validated end (count check), publication. Dropping
    /// the future (cancellation) removes the staged shard.
    pub fn run(
        &self,
        part: ResultPart,
    ) -> impl Future<Output = Result<ShardReceipt, PartFailure>> + 'static {
        let runner = self.clone();
        async move {
            let fail = |error, rows_discarded| PartFailure {
                error,
                rows_discarded,
            };
            let mut records: ResultPartRows<ContactExportRow> = runner
                .parts
                .open(&part.link)
                .await
                .map_err(|e| fail(PartError::Sf(e), 0))?;
            let mut shard = runner
                .staging
                .begin(&part)
                .map_err(|e| fail(PartError::Sink(e), 0))?;
            let mut rows = 0u64;
            let mut batch = Vec::with_capacity(SHARD_BATCH);
            loop {
                let next = records.next().await;
                let done = match next {
                    Some(Ok(row)) => {
                        rows += 1;
                        batch.push(row);
                        false
                    }
                    // A read error, a limit, or a count mismatch at the end of input.
                    Some(Err(e)) => return Err(fail(PartError::Sf(e), rows)),
                    None => true,
                };
                if batch.len() == SHARD_BATCH || (done && !batch.is_empty()) {
                    let bytes = runner
                        .writer
                        .to_vec(Rows::new(batch.drain(..)))
                        .map_err(|e| fail(PartError::Encode(Box::new(CsvError::from(e))), rows))?;
                    shard
                        .append(&bytes)
                        .map_err(|e| fail(PartError::Sink(e), rows))?;
                }
                if done {
                    break;
                }
            }
            // Publish only after the SDK validated the end of the part.
            let complete = records.finish().map_err(|e| fail(PartError::Sf(e), rows))?;
            shard
                .publish(complete.records, complete.declared_records)
                .map_err(|e| fail(PartError::Sink(e), rows))
        }
    }
}

/// A part that failed.
#[derive(Debug)]
pub struct FailedPart {
    /// The part.
    pub part: ResultPart,
    /// The error.
    pub error: PartError,
    /// Parsed rows discarded with its staged shard.
    pub rows_discarded: u64,
}

/// The discovery side of a report.
#[derive(Debug, Default)]
pub struct DiscoveryState {
    /// Manifest calls in this run.
    pub calls: u64,
    /// Whether every manifest was read (the last one had no next link).
    pub complete: bool,
    /// The manifest failure that ended discovery, if any.
    pub error: Option<SfError>,
    /// Where discovery continues (`None` when complete).
    pub resume: Option<Resume<ManifestCursor>>,
}

/// How the export ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportStatus {
    /// Every part published and combined.
    Complete,
    /// Some work remains: see the report's parts, discovery and restart.
    Incomplete,
    /// The job was still running after every allowed poll; nothing was downloaded.
    JobStillRunning,
}

/// What a run did.
#[derive(Debug)]
pub struct ExportReport {
    /// The job.
    pub job: QueryJobId,
    /// The API version of every request of the job.
    pub api_version: ApiVersion,
    /// The job's dialect.
    pub dialect: BulkDialect,
    /// The job's `numberRecordsProcessed`.
    pub records_processed: Option<u64>,
    /// How it ended.
    pub status: ExportStatus,
    /// Published shards emitted in order (and receipts reused from a restart), by ordinal.
    pub completed: Vec<ShardReceipt>,
    /// Published shards that completed but were not emitted because the traversal stopped
    /// behind an earlier part (reused by the restart).
    pub retained: Vec<ShardReceipt>,
    /// Failed parts.
    pub failed: Vec<FailedPart>,
    /// Parts whose download was dropped when the traversal stopped.
    pub cancelled: Vec<ResultPart>,
    /// Parts discovered but never started.
    pub queued: Vec<ResultPart>,
    /// Discovery.
    pub discovery: DiscoveryState,
    /// The pagination outcome of the last traversal of this run.
    pub traversal: Option<Outcome>,
    /// The combined file, when complete.
    pub output: Option<CombinedOutput>,
    /// What to pass to [`continue_export`], when incomplete.
    pub restart: Option<ExportRestart>,
}

/// Everything needed to continue an incomplete export later: the job (and the dialect it was
/// created with), the published receipts, the parts to run again and where discovery resumes.
#[derive(Clone, Debug)]
pub struct ExportRestart {
    /// The job.
    pub job: QueryJobId,
    /// Its dialect.
    pub dialect: BulkDialect,
    /// Published shards to reuse.
    pub completed: Vec<ShardReceipt>,
    /// Parts to run again (failed, cancelled, queued), by ordinal.
    pub rerun: Vec<ResultPart>,
    /// Where discovery continues; `None` when every manifest was read.
    pub discovery: Option<Resume<ManifestCursor>>,
}

/// Runs SF-B: creates the export job (**job creation in the org**: live use needs a sandbox
/// opt-in), waits for it with `wait(poll)` between polls, downloads the parts and combines them.
///
/// # Errors
///
/// [`ExportError`] when the job cannot be created, fails, or the staging directory cannot be
/// used. Part and discovery failures are reported in the [`ExportReport`].
pub async fn bulk_csv_export<W, F>(
    ctx: &OrgContext,
    input: &ExportInput,
    wait: W,
) -> Result<ExportReport, ExportError>
where
    W: FnMut(u32) -> F,
    F: Future<Output = ()>,
{
    let soql = export_soql(input)?;
    let (job, _) = BulkQueryJob::create(ctx, &soql, input.dialect)
        .await
        .map_err(ExportError::Job)?;
    let restart = ExportRestart {
        job: job.id().clone(),
        dialect: input.dialect,
        completed: Vec::new(),
        rerun: Vec::new(),
        discovery: Some(Resume::Start),
    };
    run(job, restart, input, wait).await
}

/// Continues an incomplete export from its [`ExportRestart`] (the job must still exist; result
/// locators stay valid as long as it does).
///
/// # Errors
///
/// As [`bulk_csv_export`].
pub async fn continue_export<W, F>(
    ctx: &OrgContext,
    restart: ExportRestart,
    input: &ExportInput,
    wait: W,
) -> Result<ExportReport, ExportError>
where
    W: FnMut(u32) -> F,
    F: Future<Output = ()>,
{
    let job = BulkQueryJob::attach(ctx, restart.job.clone(), restart.dialect);
    run(job, restart, input, wait).await
}

async fn run<W, F>(
    job: BulkQueryJob,
    restart: ExportRestart,
    input: &ExportInput,
    wait: W,
) -> Result<ExportReport, ExportError>
where
    W: FnMut(u32) -> F,
    F: Future<Output = ()>,
{
    let info = match job
        .wait_complete(input.max_polls, wait)
        .await
        .map_err(ExportError::Job)?
    {
        JobWait::Complete(info) => info,
        JobWait::StillRunning(info) => {
            return Ok(ExportReport {
                job: job.id().clone(),
                api_version: info.api_version,
                dialect: job.dialect(),
                records_processed: info.number_records_processed,
                status: ExportStatus::JobStillRunning,
                completed: restart.completed.clone(),
                retained: Vec::new(),
                failed: Vec::new(),
                cancelled: Vec::new(),
                queued: Vec::new(),
                discovery: DiscoveryState::default(),
                traversal: None,
                output: None,
                restart: Some(restart),
            });
        }
    };
    let staging = ShardStaging::new(&input.staging).map_err(ExportError::Staging)?;
    let runner = PartRunner::new(&job, staging, input.max_part_bytes)?;
    let options = Options::new(input.job_budget)
        .window(input.window)
        .order(input.order)
        .on_error(input.on_error);

    let mut report = ExportReport {
        job: job.id().clone(),
        api_version: info.api_version,
        dialect: job.dialect(),
        records_processed: info.number_records_processed,
        status: ExportStatus::Incomplete,
        completed: Vec::new(),
        retained: Vec::new(),
        failed: Vec::new(),
        cancelled: Vec::new(),
        queued: Vec::new(),
        discovery: DiscoveryState {
            complete: restart.discovery.is_none(),
            resume: restart.discovery.clone(),
            ..DiscoveryState::default()
        },
        traversal: None,
        output: None,
        restart: None,
    };

    // Reuse intact receipts; run the others again.
    let mut rerun = restart.rerun.clone();
    for receipt in restart.completed {
        if receipt.is_intact() {
            report.completed.push(receipt);
        } else {
            rerun.push(receipt.part);
        }
    }
    rerun.sort_by_key(|p| p.ordinal);

    // Phase A: parts already known.
    let mut stopped = false;
    if !rerun.is_empty() {
        let mut parts = independent(rerun, options, |part| runner.run(part));
        while let Some(outcome) = parts.next().await {
            match outcome {
                Ok(done) => report.completed.push(done.value),
                Err(failed) => report.failed.push(FailedPart {
                    part: failed.key,
                    error: failed.value.error,
                    rows_discarded: failed.value.rows_discarded,
                }),
            }
        }
        let finished = parts.finish();
        report.traversal = Some(finished.outcome);
        stopped = !matches!(
            finished.outcome,
            Outcome::Exhausted | Outcome::ExhaustedWithFailures
        );
        report
            .retained
            .extend(finished.retained.into_iter().map(|k| k.value));
        for failed in finished.unemitted_errors {
            report.failed.push(FailedPart {
                part: failed.key,
                error: failed.value.error,
                rows_discarded: failed.value.rows_discarded,
            });
        }
        for entry in finished.unfinished {
            if entry.value == Unfinished::Cancelled {
                report.cancelled.push(entry.key);
            }
        }
        if let Some(rest) = finished.rest {
            report.queued.extend(rest);
        }
    }

    // Phase B: the rest of discovery.
    if let (false, Some(from)) = (stopped, restart.discovery.clone()) {
        let manifests: ResultManifests = job.result_manifests();
        let job_options = JobOptions::new(input.discovery_budget, options).backlog(input.backlog);
        let mut jobs = manifest_jobs(
            from,
            job_options,
            |key| manifests.fetch(key),
            |part| runner.run(part),
        );
        while let Some(outcome) = jobs.next().await {
            match outcome {
                Ok(done) => report.completed.push(done.value),
                Err(JobsError::Job(failed)) => report.failed.push(FailedPart {
                    part: failed.key,
                    error: failed.value.error,
                    rows_discarded: failed.value.rows_discarded,
                }),
                Err(JobsError::Discovery(error)) => report.discovery.error = Some(error),
            }
        }
        let finished = jobs.finish();
        report.traversal = Some(finished.outcome);
        report.discovery.calls = finished.discovery.calls;
        report.discovery.complete = finished.discovery.resume.is_none();
        report.discovery.resume = finished.discovery.resume;
        report
            .retained
            .extend(finished.retained.into_iter().map(|k| k.value));
        for failed in finished.unemitted_errors {
            report.failed.push(FailedPart {
                part: failed.key,
                error: failed.value.error,
                rows_discarded: failed.value.rows_discarded,
            });
        }
        for entry in finished.unfinished {
            if entry.value == Unfinished::Cancelled {
                report.cancelled.push(entry.key);
            }
        }
        report.queued.extend(finished.queued);
    }

    report.completed.sort_by_key(|r| r.part.ordinal);
    report.retained.sort_by_key(|r| r.part.ordinal);
    report.failed.sort_by_key(|f| f.part.ordinal);
    report.cancelled.sort_by_key(|p| p.ordinal);
    report.queued.sort_by_key(|p| p.ordinal);

    let everything_done = report.discovery.complete
        && report.failed.is_empty()
        && report.cancelled.is_empty()
        && report.queued.is_empty()
        && report.retained.is_empty();
    if everything_done {
        let output = combine(runner.header(), &report.completed, &input.output)
            .map_err(ExportError::Combine)?;
        report.output = Some(output);
        report.status = ExportStatus::Complete;
    } else {
        let mut rerun: Vec<ResultPart> = report
            .failed
            .iter()
            .map(|f| f.part.clone())
            .chain(report.cancelled.iter().cloned())
            .chain(report.queued.iter().cloned())
            .collect();
        rerun.sort_by_key(|p| p.ordinal);
        let mut completed = report.completed.clone();
        completed.extend(report.retained.iter().cloned());
        completed.sort_by_key(|r| r.part.ordinal);
        report.restart = Some(ExportRestart {
            job: job.id().clone(),
            dialect: job.dialect(),
            completed,
            rerun,
            discovery: report.discovery.resume.clone(),
        });
    }
    Ok(report)
}
