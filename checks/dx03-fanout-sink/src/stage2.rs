//! Bounded parallel result downloads with incremental, transactional part sinks.
//!
//! Resume tokens own their job, sink and completed receipts. They are in-memory tokens,
//! not a crash-recovery journal; do not drop an export future if you need its resume token.
//!
//! Each part is read by the SDK's validated part reader (`ResultPartReader`: the job's
//! dialect, the selected header, the link's `maxRecords`, provider error documents and the
//! declared record count). This module stages rows, aborts on any failure and publishes a
//! part only after the SDK returned its validated completion.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use concord_example_salesforce_sdk::{
    OrgContext, ProtocolError, ProtocolErrorKind, SfError,
    bulk::{BulkDialect, JobState},
    bulk_query::{BulkQueryJob, ManifestCursor, ResultPart, ResultPartReader},
    ids::{QueryJobId, RecordId},
    soql::FieldPath,
};
use concord_pagination::{
    Budget, JobOptions, JobsError, OnError, Options, Order, Outcome, Resume, independent,
    manifest_jobs,
};
use serde::Serialize;

use crate::stage1::{ACCOUNT_COLUMNS, AccountRow, ExportError};

/// A sink transaction for one part. Dropping it must leave no published partial part.
///
/// Methods are synchronous: implementations must not wait for other downloads. The
/// downloader calls `push` once per decoded row and `publish` only after validated EOF.
pub trait StagedPart {
    /// Stage one row; do not publish it yet.
    fn push(&mut self, row: &AccountRow) -> io::Result<()>;
    /// Atomically publish the whole part. An error must leave it unpublished.
    fn publish(self) -> io::Result<()>;
}

/// A caller-supplied destination supporting separate transactions for concurrent parts.
pub trait PartSink {
    /// The per-part transaction, including its abort-on-drop behavior.
    type Part: StagedPart;
    /// Begin staging a fresh attempt at an ordinal. Never overwrite a completed part.
    fn begin(&self, ordinal: u64) -> io::Result<Self::Part>;
}

/// Bounds for one invocation of the export or its resume method.
#[derive(Clone, Copy, Debug)]
pub struct RunLimits {
    /// Maximum simultaneous downloads, including parsing and sink work through EOF.
    pub concurrency: NonZeroUsize,
    /// Maximum result requests across retries and newly discovered parts in this pass.
    pub result_requests: NonZeroUsize,
    /// Maximum manifest requests in this pass; discovery has one separate slot.
    pub manifest_requests: NonZeroUsize,
}

impl RunLimits {
    fn jobs(self, budget: NonZeroUsize) -> Options {
        Options::new(Budget::new(budget))
            .window(self.concurrency)
            .order(Order::Completion)
            .on_error(OnError::Continue)
    }
}

/// Small evidence of a published part; never contains its rows.
#[derive(Debug)]
pub struct PartReceipt {
    /// Stable discovery ordinal, also used when resuming.
    pub ordinal: u64,
    /// Rows staged and published.
    pub rows: u64,
    /// CSV bytes consumed.
    pub bytes: u64,
    /// Provider count, validated when present; absent is permitted for parallel results.
    pub declared_records: Option<u64>,
}

/// A download, parse, validation or sink failure.
#[derive(Debug)]
pub enum PartError {
    /// Original typed SDK error, including mid-body `SfError::Read`.
    Salesforce(SfError),
    /// The sink refused to begin, write or publish its transaction.
    Sink(io::Error),
}

impl fmt::Display for PartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Salesforce(error) => write!(f, "result part: {error}"),
            Self::Sink(error) => write!(f, "result part sink: {error}"),
        }
    }
}

impl Error for PartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Salesforce(error) => Some(error),
            Self::Sink(error) => Some(error),
        }
    }
}

impl From<SfError> for PartError {
    fn from(error: SfError) -> Self {
        Self::Salesforce(error)
    }
}

impl From<io::Error> for PartError {
    fn from(error: io::Error) -> Self {
        Self::Sink(error)
    }
}

/// A failed part's identity and original error from the most recent pass.
#[derive(Debug)]
pub struct PartFailure {
    /// The descriptor to re-download, not a sequential-results locator to follow.
    pub part: ResultPart,
    /// Why this attempt did not publish.
    pub error: PartError,
}

/// Completion ledger and an owned resume token tied to the same job and sink.
pub struct ExportReport<S> {
    /// The job's validated part reader (it holds the job).
    parts: ResultPartReader<AccountRow>,
    sink: S,
    completed: BTreeMap<u64, PartReceipt>,
    redo: Vec<ResultPart>,
    discovery: Option<Resume<ManifestCursor>>,
    /// Part failures in the most recent pass. Resume replaces these diagnostics.
    pub failures: Vec<PartFailure>,
    /// Discovery failure in the most recent pass, if any.
    pub discovery_error: Option<SfError>,
    /// Result factory invocations in the most recent pass; excludes job-info and manifests.
    pub result_requests: u64,
    /// Manifest requests in the most recent pass.
    pub discovery_requests: u64,
    /// Paginator outcomes for the retry phase and/or discovery phase of this pass.
    pub outcomes: Vec<Outcome>,
}

impl<S: PartSink> ExportReport<S> {
    /// All published receipts in provider order, including earlier passes.
    pub fn completed(&self) -> impl Iterator<Item = &PartReceipt> {
        self.completed.values()
    }

    /// Known unfinished descriptors: failures, cancellations and unattempted queued work.
    pub fn redo(&self) -> &[ResultPart] {
        &self.redo
    }

    /// The next manifest to discover, if some work remains unknown.
    pub fn pending_discovery(&self) -> Option<&Resume<ManifestCursor>> {
        self.discovery.as_ref()
    }

    /// The same sink this token has always used.
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// True only after all manifests and all of their parts completed successfully.
    pub fn is_complete(&self) -> bool {
        self.redo.is_empty() && self.discovery.is_none()
    }

    /// Retry only known unfinished parts, then continue any unfinished discovery.
    ///
    /// Completed parts are never downloaded again. Independent failures continue within
    /// the supplied bounds; the returned report carries their typed errors. No automatic
    /// retry occurs within a pass. Memory grows with descriptors/receipts, not row count.
    pub async fn resume(mut self, limits: RunLimits) -> Self {
        self.failures.clear();
        self.discovery_error = None;
        self.result_requests = 0;
        self.discovery_requests = 0;
        self.outcomes.clear();
        let parts = &self.parts;
        let sink = &self.sink;

        if !self.redo.is_empty() {
            let mut jobs = independent(
                std::mem::take(&mut self.redo),
                limits.jobs(limits.result_requests),
                |part| download_part(parts, sink, part),
            );
            while let Some(result) = jobs.next().await {
                match result {
                    Ok(done) => {
                        self.completed.insert(done.key.ordinal, done.value);
                    }
                    Err(failed) => self.failures.push(PartFailure {
                        part: failed.key,
                        error: failed.value,
                    }),
                }
            }
            // Lossless: every part of the report is moved out, nothing cloned or taken.
            let resume = jobs.finish().into_resume();
            self.result_requests += resume.progress.admitted;
            self.outcomes.push(resume.outcome);
            for failed in resume.unemitted_errors {
                self.failures.push(PartFailure {
                    part: failed.key,
                    error: failed.value,
                });
            }
            for done in resume.retained {
                self.completed.insert(done.key.ordinal, done.value);
            }
            // Attempted parts (failed or cancelled), then the never-admitted rest of the plan.
            self.redo = resume
                .rerun
                .into_iter()
                .map(|entry| entry.key)
                .chain(resume.rest.into_iter().flatten())
                .collect();
        }

        // The two phases share one result-request budget and never overlap their windows.
        let remaining = limits.result_requests.get() - self.result_requests as usize;
        if let Some(remaining) = NonZeroUsize::new(remaining)
            && let Some(from) = self.discovery.take()
        {
            let manifests = parts.job().result_manifests();
            let mut jobs = manifest_jobs(
                from,
                JobOptions::new(
                    Budget::new(limits.manifest_requests),
                    limits.jobs(remaining),
                ),
                |key| manifests.fetch(key),
                |part| download_part(parts, sink, part),
            );
            while let Some(result) = jobs.next().await {
                match result {
                    Ok(done) => {
                        self.completed.insert(done.key.ordinal, done.value);
                    }
                    Err(JobsError::Job(failed)) => self.failures.push(PartFailure {
                        part: failed.key,
                        error: failed.value,
                    }),
                    Err(JobsError::Discovery(error)) => self.discovery_error = Some(error),
                }
            }
            let resume = jobs.finish().into_resume();
            self.result_requests += resume.progress.jobs.admitted;
            self.discovery_requests = resume.discovery.calls;
            self.outcomes.push(resume.outcome);
            for failed in resume.unemitted_errors {
                self.failures.push(PartFailure {
                    part: failed.key,
                    error: failed.value,
                });
            }
            for done in resume.retained {
                self.completed.insert(done.key.ordinal, done.value);
            }
            // Known work (attempted, then queued), in discovery order; then undiscovered work.
            self.redo.extend(
                resume
                    .rerun
                    .into_iter()
                    .map(|entry| entry.key)
                    .chain(resume.queued),
            );
            self.discovery = resume.discovery.resume;
        }
        self
    }
}

/// Export the same Account projection as Stage 1, through parallel-results manifests.
///
/// Reads job information once to discover the dialect. Subsequent failures and bounds
/// produce a resumable report, not a truncated success. Sink ownership travels with it.
///
/// # Errors
///
/// Setup errors preserve `SfError`; missing dialect, wrong job/version, or an unfinished
/// job are refused before discovery. Download/discovery errors are in the returned report.
pub async fn export<S: PartSink>(
    ctx: &OrgContext,
    job_id: QueryJobId,
    sink: S,
    limits: RunLimits,
) -> Result<ExportReport<S>, ExportError> {
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
    let invalid = if info.id != job_id {
        Some(ProtocolErrorKind::JobIdChanged)
    } else if info.api_version != ctx.version() {
        Some(ProtocolErrorKind::ApiVersionChanged)
    } else if info.state != JobState::JobComplete {
        Some(ProtocolErrorKind::JobNotComplete)
    } else {
        None
    };
    if let Some(kind) = invalid {
        return Err(SfError::Protocol(Box::new(ProtocolError::new("stage2.export", kind))).into());
    }
    let dialect = BulkDialect::new(
        info.column_delimiter.ok_or(ExportError::MissingDialect)?,
        info.line_ending.ok_or(ExportError::MissingDialect)?,
    );
    let columns =
        ACCOUNT_COLUMNS.map(|name| FieldPath::parse(name).expect("static API field name"));
    let job = BulkQueryJob::attach(ctx, job_id, dialect);
    let parts = ResultPartReader::new(&job, &columns).map_err(SfError::from)?;
    Ok(ExportReport {
        parts,
        sink,
        completed: BTreeMap::new(),
        redo: Vec::new(),
        discovery: Some(Resume::Start),
        failures: Vec::new(),
        discovery_error: None,
        result_requests: 0,
        discovery_requests: 0,
        outcomes: Vec::new(),
    }
    .resume(limits)
    .await)
}

async fn download_part<S: PartSink>(
    parts: &ResultPartReader<AccountRow>,
    sink: &S,
    part: ResultPart,
) -> Result<PartReceipt, PartError> {
    // Status/provider error document, head, `Sforce-*` headers and `maxRecords`: the SDK's.
    let mut rows = parts.open(&part.link).await?;
    let mut staged = sink.begin(part.ordinal)?;
    while let Some(row) = rows.next().await {
        staged.push(&row?)?;
    }
    // Only a validated end of input (declared count checked) completes; `staged` aborts on
    // drop for every error above or here.
    let complete = rows.finish()?;
    staged.publish()?;
    Ok(PartReceipt {
        ordinal: part.ordinal,
        rows: complete.records,
        bytes: complete.bytes,
        declared_records: complete.declared_records,
    })
}

/// A file sink in a fresh directory: one JSON-lines file per completed part.
///
/// Each writer has a bounded `BufWriter`. File I/O is synchronous. Publication uses a
/// same-directory hard link (atomic and refuses overwrite), after flushing and syncing
/// the staged file. The staging link is then removed by `Drop`. This is part-level
/// publication, not a crash-durable export journal or a whole-export transaction.
pub struct FileSink {
    directory: PathBuf,
}

impl FileSink {
    /// Create a new export directory. Refuses an existing path to isolate this export.
    pub fn create(directory: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir(directory.as_ref())?;
        Ok(Self {
            directory: directory.as_ref().to_owned(),
        })
    }

    /// The published file for a stable part ordinal; absent until validated completion.
    pub fn published_path(&self, ordinal: u64) -> PathBuf {
        self.directory.join(format!("part-{ordinal:020}.jsonl"))
    }

    /// Copy completed parts incrementally in receipt order. `ExportReport::completed`
    /// yields provider order; check `is_complete()` before calling this a full export.
    pub fn copy_completed<'a>(
        &self,
        receipts: impl IntoIterator<Item = &'a PartReceipt>,
        output: &mut impl Write,
    ) -> io::Result<u64> {
        let mut bytes = 0;
        for receipt in receipts {
            bytes += io::copy(
                &mut File::open(self.published_path(receipt.ordinal))?,
                output,
            )?;
        }
        Ok(bytes)
    }
}

/// File-backed part transaction. Its drop guard also handles parser failure/cancellation.
pub struct FilePart {
    writer: Option<BufWriter<File>>,
    staged: PathBuf,
    published: PathBuf,
}

impl PartSink for FileSink {
    type Part = FilePart;

    fn begin(&self, ordinal: u64) -> io::Result<FilePart> {
        let staged = self.directory.join(format!("part-{ordinal:020}.staging"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
        Ok(FilePart {
            writer: Some(BufWriter::new(file)),
            staged,
            published: self.published_path(ordinal),
        })
    }
}

// Stage 1's row remains unchanged (Deserialize only); serialize a borrowed view.
#[derive(Serialize)]
struct StoredRow<'a> {
    #[serde(rename = "Id")]
    id: &'a RecordId,
    #[serde(rename = "Name")]
    name: &'a str,
    #[serde(rename = "NumberOfEmployees")]
    employees: Option<u64>,
}

impl StagedPart for FilePart {
    fn push(&mut self, row: &AccountRow) -> io::Result<()> {
        let writer = self.writer.as_mut().expect("live staging transaction");
        serde_json::to_writer(
            &mut *writer,
            &StoredRow {
                id: &row.id,
                name: &row.name,
                employees: row.number_of_employees,
            },
        )
        .map_err(io::Error::other)?;
        writer.write_all(b"\n")
    }

    fn publish(mut self) -> io::Result<()> {
        let mut writer = self.writer.take().expect("live staging transaction");
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        fs::hard_link(&self.staged, &self.published)
    }
}

impl Drop for FilePart {
    fn drop(&mut self) {
        self.writer.take();
        // Best-effort cleanup: even if unlink fails, a .staging file is never complete.
        let _ = fs::remove_file(&self.staged);
    }
}
