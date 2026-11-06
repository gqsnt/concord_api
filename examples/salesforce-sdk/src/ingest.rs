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

use concord::Incoming;
use concord::bytes::Bytes;
use concord::csv::{CsvRecords, RawRecord};

use crate::bulk::{BulkDialect, CreateIngestJob, IngestJobInfo, JobState, JobStateChange};
use crate::context::OrgContext;
use crate::csv::{CsvError, SalesforceCsv, SpooledCsv};
use crate::error::{JobFailure, ProtocolError, ProtocolErrorKind, SfError};
use crate::ids::{IngestJobId, RecordId};
use crate::transport::{operation_name, prepared, send};

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

    /// Uploads a spooled body ([`SalesforceCsv::spool`] / [`SalesforceCsv::spool_stream`] with
    /// this job's dialect). **Writes to the org.**
    ///
    /// # Errors
    ///
    /// [`SfError`].
    pub async fn upload_spooled(&self, spooled: SpooledCsv) -> Result<(), SfError> {
        self.upload(spooled.into_body()).await
    }

    /// The CSV dialect of this job's bodies.
    pub fn csv(&self) -> SalesforceCsv {
        SalesforceCsv::new(self.dialect)
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
                    return Err(SfError::from(JobFailure::new(
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

    /// Requests a result file (its declared `-> stream` operation): the 2xx CSV body, unread,
    /// with the endpoint's identity, for a reader of the job's dialect.
    ///
    /// # Errors
    ///
    /// [`SfError::Api`] for a non-2xx status (its error document read), or a transport
    /// [`SfError`].
    pub async fn open_result_file(&self, file: ResultFile) -> Result<Incoming, SfError> {
        let job = self.ctx.api().ingest().job(self.id.clone());
        let op = match file {
            ResultFile::Successful => job.successful_results().prepare(),
            ResultFile::Failed => job.failed_results().prepare(),
            ResultFile::Unprocessed => job.unprocessed_records().prepare(),
        };
        send(prepared(op)?).await
    }

    /// Opens a result file as typed result rows (the `sf__` columns mapped, the uploaded fields
    /// kept by name). The file is read with the job's dialect.
    ///
    /// # Errors
    ///
    /// [`SfError::Api`] for a non-2xx status; [`SfError::Open`] for a refused head.
    pub async fn result_rows(&self, file: ResultFile) -> Result<IngestResultRows, SfError> {
        let body = self.open_result_file(file).await?;
        let operation = operation_name(body.operation());
        let reader = self.csv().raw_reader().map_err(SfError::from)?;
        match reader.open_raw(body) {
            Ok(records) => Ok(IngestResultRows {
                file,
                operation,
                records,
                layout: None,
            }),
            Err(error) => Err(SfError::from_open(error).await),
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

/// One row of an ingest result file. `Debug` shows the id, the created flag, whether an error is
/// present and the number of fields; values and the error text are withheld.
#[derive(Clone)]
pub struct IngestResultRow {
    /// `sf__Id` (empty for a failed insert, absent in unprocessed records).
    pub id: Option<RecordId>,
    /// `sf__Created` (successful results).
    pub created: Option<bool>,
    error: Option<String>,
    /// The uploaded fields, by header name, in file order.
    pub fields: Vec<(String, String)>,
}

impl IngestResultRow {
    /// `sf__Error` (failed results), **unredacted**: "error code and message".
    pub fn error_unredacted(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The uploaded field `name`.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

impl fmt::Debug for IngestResultRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IngestResultRow")
            .field("id", &self.id)
            .field("created", &self.created)
            .field("error", &self.error.as_ref().map(|_| "<withheld>"))
            .field("fields", &self.fields.len())
            .finish()
    }
}

/// The rows of one result file, read incrementally.
pub struct IngestResultRows {
    file: ResultFile,
    operation: &'static str,
    records: CsvRecords<RawRecord>,
    layout: Option<Layout>,
}

struct Layout {
    id: Option<usize>,
    created: Option<usize>,
    error: Option<usize>,
    names: Vec<String>,
}

impl IngestResultRows {
    /// The next row; `None` at the end. The header is checked before the first row: successful
    /// results need `sf__Id` and `sf__Created`, failed results `sf__Id` and `sf__Error`,
    /// unprocessed records no `sf__` column.
    pub async fn next(&mut self) -> Option<Result<IngestResultRow, SfError>> {
        let record = self.records.next().await;
        if self.layout.is_none() {
            match self.layout() {
                Ok(layout) => self.layout = layout,
                Err(e) => return Some(Err(e)),
            }
        }
        let record = match record? {
            Ok(record) => record,
            Err(e) => return Some(Err(e.into())),
        };
        let layout = self.layout.as_ref()?;
        let values = match record.to_strings() {
            Ok(values) => values,
            Err(e) => return Some(Err(SfError::from(CsvError::Encode(e)))),
        };
        let mut row = IngestResultRow {
            id: None,
            created: None,
            error: None,
            fields: Vec::new(),
        };
        for (i, value) in values.into_iter().enumerate() {
            if Some(i) == layout.id {
                if !value.is_empty() {
                    match RecordId::new(value) {
                        Ok(id) => row.id = Some(id),
                        Err(_) => return Some(Err(self.header_error())),
                    }
                }
            } else if Some(i) == layout.created {
                row.created = match value.as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => return Some(Err(self.header_error())),
                };
            } else if Some(i) == layout.error {
                row.error = Some(value);
            } else {
                row.fields.push((layout.names[i].clone(), value));
            }
        }
        Some(Ok(row))
    }

    /// Which file.
    pub fn file(&self) -> ResultFile {
        self.file
    }

    /// The declared endpoint that produced the file (named by its errors).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    fn header_error(&self) -> SfError {
        ProtocolError::new(self.operation, ProtocolErrorKind::ResultFileLayout).into()
    }

    fn layout(&self) -> Result<Option<Layout>, SfError> {
        let Some(headers) = self.records.headers() else {
            return Ok(None);
        };
        let names = headers.to_strings().map_err(|_| self.header_error())?;
        let find = |name: &str| names.iter().position(|n| n == name);
        let (id, created, error) = (find("sf__Id"), find("sf__Created"), find("sf__Error"));
        let ok = match self.file {
            ResultFile::Successful => id.is_some() && created.is_some() && error.is_none(),
            ResultFile::Failed => id.is_some() && error.is_some() && created.is_none(),
            ResultFile::Unprocessed => id.is_none() && created.is_none() && error.is_none(),
        };
        let others_ok = names
            .iter()
            .filter(|n| !n.starts_with("sf__"))
            .all(|n| crate::soql::FieldPath::parse(n).is_ok());
        if !ok || !others_ok {
            return Err(self.header_error());
        }
        Ok(Some(Layout {
            id,
            created,
            error,
            names,
        }))
    }
}

impl fmt::Debug for IngestResultRows {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IngestResultRows")
            .field("file", &self.file)
            .field("records", &self.records)
            .finish()
    }
}
