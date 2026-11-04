//! Bulk API 2.0 ingest jobs: create, upload, close, poll and the three result files.
//!
//! # Upload framing
//!
//! `PUT jobs/ingest/{id}/batches` takes `Content-Type: text/csv` and "CSV data that does not in
//! total exceed 150 MB of base64 encoded content … upload data that does not exceed 100 MB". The
//! documentation states no support for chunked transfer encoding, and a size limit that has to
//! be respected before sending. This SDK therefore uploads a **complete body of known length**
//! (a `Content-Length` request), produced by the public CSV encoder into a bounded in-memory
//! spool ([`crate::csv::SpooledCsv`], at most [`MAX_UPLOAD_BYTES`] or a lower caller limit). The
//! cost is one copy of the encoded data in memory before the request starts; this is not
//! streaming serialization, and the SDK does not claim it is. The incremental, chunked
//! encoder route (`CsvWriter::upload`) is exercised separately against a labelled generic
//! transport fixture, not presented as Salesforce evidence.
//!
//! Every ingest operation **writes to the org** (job creation, upload, close); live use needs the
//! caller's explicit sandbox mutation opt-in.

use std::fmt;
use std::future::Future;
use std::num::NonZeroUsize;

use concord::bytes::Bytes;
use concord::reqwest;

use crate::bulk::{BulkDialect, CreateIngestJob, IngestJobInfo, JobState, JobStateChange};
use crate::context::OrgContext;
use crate::error::{JobFailure, ProtocolError, ProtocolErrorKind, SfError};
use crate::ids::IngestJobId;
use crate::transport::{prepared, send};

/// The documented raw upload size limit per request: 100 MB (150 MB of base64 content).
pub const MAX_UPLOAD_BYTES: usize = 100 * 1000 * 1000;

/// An ingest job of one org, version and dialect.
#[derive(Clone)]
pub struct IngestJob {
    ctx: OrgContext,
    id: IngestJobId,
    dialect: BulkDialect,
}

/// The outcome of waiting for an ingest job.
#[derive(Debug)]
pub enum IngestWait {
    /// `JobComplete`.
    Complete(IngestJobInfo),
    /// `Failed` (processed records are not rolled back; read the result files).
    Failed(IngestJobInfo),
    /// Still processing after every allowed poll.
    StillRunning(IngestJobInfo),
}

/// Which result file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultFile {
    /// `successfulResults/`: `sf__Id`, `sf__Created`, then the uploaded fields.
    Successful,
    /// `failedResults/`: `sf__Id`, `sf__Error`, then the uploaded fields.
    Failed,
    /// `unprocessedrecords/`: the uploaded fields.
    Unprocessed,
}

impl ResultFile {
    /// The operation label CSV readers name in their errors.
    pub fn operation(self) -> &'static str {
        match self {
            ResultFile::Successful => "ingest.job.successful_results",
            ResultFile::Failed => "ingest.job.failed_results",
            ResultFile::Unprocessed => "ingest.job.unprocessed_records",
        }
    }
}

impl IngestJob {
    /// Creates an ingest job (`POST jobs/ingest`). **Writes to the org.**
    ///
    /// # Errors
    ///
    /// [`SfError`]; a document with another version or dialect, a state other than `Open`, or a
    /// `contentUrl` that is not this job's upload resource is refused.
    pub async fn create(
        ctx: &OrgContext,
        body: CreateIngestJob,
    ) -> Result<(Self, IngestJobInfo), SfError> {
        let dialect = body.dialect();
        let info = send(prepared(ctx.api().ingest().create(body).prepare())?).await?;
        let job = Self {
            ctx: ctx.clone(),
            id: info.id.clone(),
            dialect,
        };
        job.check("ingest.create", &info)?;
        if info.state != JobState::Open {
            return Err(ProtocolError::new("ingest.create", ProtocolErrorKind::JobNotOpen).into());
        }
        if let Some(url) = &info.content_url {
            ctx.content_url(&job.id, url)?;
        }
        Ok((job, info))
    }

    /// The job id.
    pub fn id(&self) -> &IngestJobId {
        &self.id
    }

    /// The dialect the job was created with.
    pub fn dialect(&self) -> BulkDialect {
        self.dialect
    }

    /// The context.
    pub fn context(&self) -> &OrgContext {
        &self.ctx
    }

    /// Uploads one complete CSV body of known length (`PUT .../batches`,
    /// `Content-Type: text/csv`). **Writes to the org.**
    ///
    /// # Errors
    ///
    /// [`SfError`]; a body over [`MAX_UPLOAD_BYTES`] is refused before sending
    /// ([`crate::csv::CsvError::UploadTooLarge`]).
    pub async fn upload(&self, body: Bytes) -> Result<(), SfError> {
        if body.len() > MAX_UPLOAD_BYTES {
            return Err(crate::csv::CsvError::UploadTooLarge {
                limit: MAX_UPLOAD_BYTES,
            }
            .into());
        }
        let op = self.ctx.api().ingest().job(self.id.clone()).upload(body);
        send(prepared(op.prepare())?).await
    }

    /// Closes the upload (`PATCH` with `{"state": "UploadComplete"}`). **Writes to the org.**
    ///
    /// # Errors
    ///
    /// [`SfError`].
    pub async fn close(&self) -> Result<IngestJobInfo, SfError> {
        self.set_state("ingest.job.close", JobStateChange::UPLOAD_COMPLETE)
            .await
    }

    /// Aborts the job.
    ///
    /// # Errors
    ///
    /// [`SfError`].
    pub async fn abort(&self) -> Result<IngestJobInfo, SfError> {
        self.set_state("ingest.job.abort", JobStateChange::ABORTED)
            .await
    }

    async fn set_state(
        &self,
        operation: &'static str,
        change: JobStateChange,
    ) -> Result<IngestJobInfo, SfError> {
        let op = self
            .ctx
            .api()
            .ingest()
            .job(self.id.clone())
            .set_state(change);
        let info = send(prepared(op.prepare())?).await?;
        self.check(operation, &info)?;
        Ok(info)
    }

    /// `GET jobs/ingest/{id}`, checked against the job's version and dialect.
    ///
    /// # Errors
    ///
    /// [`SfError`].
    pub async fn info(&self) -> Result<IngestJobInfo, SfError> {
        let op = self.ctx.api().ingest().job(self.id.clone()).info();
        let info = send(prepared(op.prepare())?).await?;
        self.check("ingest.job.info", &info)?;
        Ok(info)
    }

    /// Polls until `JobComplete`, `Failed` or `Aborted`, at most `max_polls` times, awaiting
    /// `wait(poll)` in between.
    ///
    /// # Errors
    ///
    /// [`SfError::Job`] for `Aborted`; any poll's [`SfError`].
    pub async fn wait_finished<W, F>(
        &self,
        max_polls: NonZeroUsize,
        mut wait: W,
    ) -> Result<IngestWait, SfError>
    where
        W: FnMut(u32) -> F,
        F: Future<Output = ()>,
    {
        let mut poll = 0u32;
        loop {
            poll += 1;
            let info = self.info().await?;
            match &info.state {
                JobState::JobComplete => return Ok(IngestWait::Complete(info)),
                JobState::Failed => return Ok(IngestWait::Failed(info)),
                JobState::Aborted => {
                    let message = info.error_message_unredacted().map(str::to_owned);
                    return Err(SfError::Job(JobFailure::new(
                        self.id.as_str(),
                        info.state.clone(),
                        message,
                    )));
                }
                _ if poll as usize >= max_polls.get() => return Ok(IngestWait::StillRunning(info)),
                _ => wait(poll).await,
            }
        }
    }

    /// Requests a result file; the CSV body is left unread for a reader (whose open refuses a
    /// non-2xx status and reads its error document).
    ///
    /// # Errors
    ///
    /// A transport [`SfError`].
    pub async fn open_result_file(&self, file: ResultFile) -> Result<reqwest::Response, SfError> {
        let job = self.ctx.api().ingest().job(self.id.clone());
        let result = match file {
            ResultFile::Successful => {
                prepared(job.successful_results().prepare())?
                    .send_raw()
                    .await
            }
            ResultFile::Failed => prepared(job.failed_results().prepare())?.send_raw().await,
            ResultFile::Unprocessed => {
                prepared(job.unprocessed_records().prepare())?
                    .send_raw()
                    .await
            }
        };
        match result {
            Ok(response) => Ok(response),
            Err(error) => Err(SfError::from_concord(error).await),
        }
    }

    fn check(&self, operation: &'static str, info: &IngestJobInfo) -> Result<(), ProtocolError> {
        if info.id != self.id {
            return Err(ProtocolError::new(
                operation,
                ProtocolErrorKind::JobIdChanged,
            ));
        }
        if info.api_version != self.ctx.version() {
            return Err(ProtocolError::new(
                operation,
                ProtocolErrorKind::ApiVersionChanged,
            ));
        }
        let delimiter_ok = info
            .column_delimiter
            .is_none_or(|d| d == self.dialect.delimiter);
        let ending_ok = info
            .line_ending
            .is_none_or(|e| e == self.dialect.line_ending);
        if !delimiter_ok || !ending_ok {
            return Err(ProtocolError::new(
                operation,
                ProtocolErrorKind::DialectChanged,
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for IngestJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IngestJob")
            .field("id", &self.id)
            .field("dialect", &self.dialect)
            .field("version", &self.ctx.version())
            .finish()
    }
}
