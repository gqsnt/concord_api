//! Bulk API 2.0 query jobs: creation with an explicit dialect, status checks that keep the job's
//! API version and dialect, polling, abort, the sequential locator chain of result sets and the
//! parallel result-pages manifest adapter for `concord_pagination::manifest_jobs`.
//!
//! # One job, one version, one dialect
//!
//! "To get query results, use the same API version that you used to create the query", and the
//! dialect (`columnDelimiter`, `lineEnding`) is fixed at creation. A [`BulkQueryJob`] holds the
//! context it was created in and the dialect it was created with; every job document it reads is
//! checked against both ([`ProtocolErrorKind::ApiVersionChanged`],
//! [`ProtocolErrorKind::DialectChanged`]), and every CSV body of the job is read with that
//! dialect ([`crate::csv::SalesforceCsv`]).
//!
//! # Result discovery
//!
//! [`ResultManifests::fetch`] is the discovery factory of `manifest_jobs`: one call reads one
//! manifest (`GET jobs/query/{id}/resultPages[?locator=..]`), validates every `resultUrl` and the
//! next link against the org, version and job (a next link repeating the requested locator is a
//! [`ProtocolErrorKind::LocatorDidNotAdvance`]), and returns them as [`ResultPart`] descriptors
//! numbered in discovery order. The key ([`ManifestCursor`]) carries the next ordinal, so a
//! discovery resumed from a report numbers its parts exactly as an uninterrupted run would.
//!
//! # Reading a result part
//!
//! [`ResultPartReader`] is the one validated reader of a job's result parts, used by both
//! routes: the collected sequential chain ([`ResultSets`]) and any streaming consumer of the
//! parallel descriptors ([`ResultPart`]). [`ResultPartReader::open`] sends the declared
//! `results` operation once (`-> stream`: the body is not read), turns a non-2xx status into the
//! provider's error document ([`SfError::Api`]), reads `Sforce-Locator` and
//! `Sforce-NumberOfRecords`, and opens the body with the job's dialect, the selected columns as
//! the exact header and the link's `maxRecords` as a record limit. [`ResultPartRows::next`]
//! yields typed rows; at the parsed end of input it checks the declared count, when present.
//! Only [`ResultPartRows::finish`] after that validated end returns a [`PartComplete`]: a part
//! dropped or abandoned early, cancelled, over a limit or ended by a read error never does.
//!
//! ```no_run
//! # use concord_example_salesforce_sdk::bulk_query::{BulkQueryJob, ResultPart, ResultPartReader};
//! # use concord_example_salesforce_sdk::{csv::ContactExportRow, soql::FieldPath, SfError};
//! # async fn run(job: &BulkQueryJob, part: ResultPart, columns: &[FieldPath]) -> Result<(), SfError> {
//! let reader = ResultPartReader::<ContactExportRow>::new(job, columns)?;
//! let mut rows = reader.open(&part.link).await?; // one request; status, head, headers checked
//! while let Some(row) = rows.next().await {
//!     let row = row?; // stage it; nothing is complete yet
//! #   let _ = row;
//! }
//! let complete = rows.finish()?; // the validated end: publish only now
//! println!("{} rows, declared {:?}", complete.records, complete.declared_records);
//! # Ok(()) }
//! ```

use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::num::NonZeroUsize;

use concord::csv::{CsvReader, CsvRecords};
use concord::stream::{Completion, Position};
use concord_pagination::{Manifest, Page};
use serde::de::DeserializeOwned;

use crate::bulk::{BulkDialect, CreateQueryJob, JobState, JobStateChange, NextLink, QueryJobInfo};
use crate::context::{OrgContext, ResultLink};
use crate::csv::{CsvError, SalesforceCsv};
use crate::error::{IncompletePart, JobFailure, ProtocolError, ProtocolErrorKind, SfError};
use crate::ids::{MaxRecords, QueryJobId, ResultLocator};
use crate::soql::{FieldPath, Soql};
use crate::transport::{operation_name, prepared, send};

const CREATE: &str = "bulk_query.create";
const INFO: &str = "bulk_query.job.info";
const MANIFEST: &str = "bulk_query.job.result_pages";

/// A query job of one org, version and dialect.
#[derive(Clone)]
pub struct BulkQueryJob {
    ctx: OrgContext,
    id: QueryJobId,
    dialect: BulkDialect,
}

/// The outcome of waiting for a job.
#[derive(Debug)]
pub enum JobWait {
    /// `JobComplete`.
    Complete(QueryJobInfo),
    /// Still `UploadComplete`/`InProgress` after every allowed poll.
    StillRunning(QueryJobInfo),
}

impl BulkQueryJob {
    /// Creates a query job for `soql` in `dialect` (`POST jobs/query`). **Creates a job in the
    /// org**: live use needs the caller's explicit sandbox opt-in.
    ///
    /// # Errors
    ///
    /// [`SfError`]; a created job whose document names another version or dialect is a
    /// [`ProtocolError`].
    pub async fn create(
        ctx: &OrgContext,
        soql: &Soql,
        dialect: BulkDialect,
    ) -> Result<(Self, QueryJobInfo), SfError> {
        let body = CreateQueryJob::new(soql, dialect);
        let info = send(prepared(ctx.api().bulk_query().create(body).prepare())?).await?;
        let job = Self {
            ctx: ctx.clone(),
            id: info.id.clone(),
            dialect,
        };
        job.check(CREATE, &info)?;
        Ok((job, info))
    }

    /// An existing job (from restart information): the context and dialect it was created with.
    pub fn attach(ctx: &OrgContext, id: QueryJobId, dialect: BulkDialect) -> Self {
        Self {
            ctx: ctx.clone(),
            id,
            dialect,
        }
    }

    /// The job id.
    pub fn id(&self) -> &QueryJobId {
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

    /// `GET jobs/query/{id}`, checked against the job's version and dialect.
    ///
    /// # Errors
    ///
    /// [`SfError`].
    pub async fn info(&self) -> Result<QueryJobInfo, SfError> {
        let op = self.ctx.api().bulk_query().job(self.id.clone()).info();
        let info = send(prepared(op.prepare())?).await?;
        self.check(INFO, &info)?;
        Ok(info)
    }

    /// Aborts the job (`PATCH` with `{"state": "Aborted"}`).
    ///
    /// # Errors
    ///
    /// [`SfError`] (Salesforce refuses to abort a completed job with `INVALIDJOBSTATE`).
    pub async fn abort(&self) -> Result<QueryJobInfo, SfError> {
        let op = self
            .ctx
            .api()
            .bulk_query()
            .job(self.id.clone())
            .abort(JobStateChange::ABORTED);
        let info = send(prepared(op.prepare())?).await?;
        self.check("bulk_query.job.abort", &info)?;
        Ok(info)
    }

    /// Polls [`info`](Self::info) up to `max_polls` times, awaiting `wait(poll_number)` between
    /// polls (the caller's timer: this crate has no runtime). No hidden retry: a failed poll is
    /// returned.
    ///
    /// # Errors
    ///
    /// [`SfError::Job`] when the job ends `Failed` or `Aborted`; any poll's [`SfError`].
    pub async fn wait_complete<W, F>(
        &self,
        max_polls: NonZeroUsize,
        mut wait: W,
    ) -> Result<JobWait, SfError>
    where
        W: FnMut(u32) -> F,
        F: Future<Output = ()>,
    {
        let mut poll = 0u32;
        loop {
            poll += 1;
            let info = self.info().await?;
            match &info.state {
                JobState::JobComplete => return Ok(JobWait::Complete(info)),
                JobState::Failed | JobState::Aborted => {
                    let message = info.error_message_unredacted().map(str::to_owned);
                    return Err(SfError::from(JobFailure::new(
                        self.id.as_str(),
                        info.state.clone(),
                        message,
                    )));
                }
                _ if poll as usize >= max_polls.get() => return Ok(JobWait::StillRunning(info)),
                _ => wait(poll).await,
            }
        }
    }

    /// The discovery factory of this job's parallel result pages.
    pub fn result_manifests(&self) -> ResultManifests {
        ResultManifests { job: self.clone() }
    }

    /// Checks a job document against this job: same id, the context's version, and the
    /// creation dialect where the document states one.
    fn check(&self, operation: &'static str, info: &QueryJobInfo) -> Result<(), ProtocolError> {
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

impl fmt::Debug for BulkQueryJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BulkQueryJob")
            .field("id", &self.id)
            .field("dialect", &self.dialect)
            .field("version", &self.ctx.version())
            .finish()
    }
}

/// What a result set's `Sforce-Locator` said about the next set.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
pub enum NextResultSet {
    /// `Sforce-Locator: null`: this is the last set.
    Last,
    /// The next set's locator.
    At(#[sensitive] ResultLocator),
    /// No header (a parallel result URL's response need not be read for it).
    Unknown,
}

/// The validated reader of one job's result parts, typed `T`: the job's dialect, the selected
/// columns as the exact header and, optionally, a byte limit per part. Both result routes use it:
/// the sequential chain ([`ResultSets`]) and the parallel descriptors ([`ResultPart`]).
///
/// Build it once per job and open each part with [`open`](Self::open). Cheap to clone (the CSV
/// configuration is shared).
#[derive(concord::Debug)]
pub struct ResultPartReader<T> {
    job: BulkQueryJob,
    reader: CsvReader,
    #[sensitive]
    _rows: PhantomData<fn() -> T>,
}

impl<T> Clone for ResultPartReader<T> {
    fn clone(&self) -> Self {
        Self {
            job: self.job.clone(),
            reader: self.reader.clone(),
            _rows: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> ResultPartReader<T> {
    /// The reader of `job`'s result parts whose header must be exactly `columns` (the query's
    /// selected fields, in order).
    ///
    /// # Errors
    ///
    /// A [`CsvError`] for a refused dialect or column list. Nothing is sent.
    pub fn new(job: &BulkQueryJob, columns: &[FieldPath]) -> Result<Self, CsvError> {
        let reader = SalesforceCsv::new(job.dialect()).result_reader(columns)?;
        Ok(Self {
            job: job.clone(),
            reader,
            _rows: PhantomData,
        })
    }

    /// At most `max` CSV bytes read from each part: more is a terminal
    /// [`Limit`](concord::stream::ReadErrorKind::Limit) read error of that part, never a
    /// completion.
    ///
    /// # Errors
    ///
    /// A [`CsvError`] for a limit the codec refuses.
    pub fn max_part_bytes(self, max: u64) -> Result<Self, CsvError> {
        let options = self.reader.options().clone().max_total_bytes(max);
        let reader = CsvReader::new(options)?;
        Ok(Self { reader, ..self })
    }

    /// The job.
    pub fn job(&self) -> &BulkQueryJob {
        &self.job
    }

    /// Requests one result part (`GET jobs/query/{id}/results[?locator=..][&maxRecords=..]`,
    /// the declared `-> stream` operation: sent once, body not read) and opens it.
    ///
    /// Before any row is read: a non-2xx status is the provider's error document
    /// ([`SfError::Api`]); `Sforce-Locator` and `Sforce-NumberOfRecords` are parsed (a malformed
    /// value is a [`ProtocolError`]; an absent one is allowed here, see
    /// [`ResultPartRows::next_set`]); the head is checked by the job's CSV dialect
    /// ([`SfError::Open`], response kept unread). The link's `maxRecords` becomes the part's
    /// record limit: a part with more rows fails with a `Limit` read error.
    ///
    /// # Errors
    ///
    /// [`SfError`] as above, a transport failure, or [`SfError::Csv`] for a refused limit.
    pub async fn open(&self, link: &ResultLink) -> Result<ResultPartRows<T>, SfError> {
        let reader = match link.max_records {
            None => self.reader.clone(),
            Some(max) => {
                let options = self
                    .reader
                    .options()
                    .clone()
                    .max_records(u64::from(max.get()));
                CsvReader::new(options).map_err(CsvError::from)?
            }
        };
        let op = self
            .job
            .ctx
            .api()
            .bulk_query()
            .job(self.job.id.clone())
            .results()
            .maybe_locator(link.locator.clone())
            .maybe_max_records(link.max_records);
        // `-> stream`: a 2xx body, unread, with the declared identity; a non-2xx status is read
        // as the provider's error document by `send`.
        let incoming = send(prepared(op.prepare())?).await?;
        let operation = operation_name(incoming.operation());
        let headers = incoming.head().headers();
        let invalid = |kind| SfError::from(ProtocolError::new(operation, kind));
        let next = match headers.get("sforce-locator").map(|v| v.to_str()) {
            None => NextResultSet::Unknown,
            Some(Ok("null")) => NextResultSet::Last,
            Some(Ok(text)) => match ResultLocator::new(text) {
                Ok(locator) => NextResultSet::At(locator),
                Err(_) => return Err(invalid(ProtocolErrorKind::LocatorHeaderInvalid)),
            },
            Some(Err(_)) => return Err(invalid(ProtocolErrorKind::LocatorHeaderInvalid)),
        };
        let declared_records = match headers.get("sforce-numberofrecords").map(|v| v.to_str()) {
            None => None,
            Some(Ok(text)) => match text.trim().parse::<u64>() {
                Ok(count) => Some(count),
                Err(_) => return Err(invalid(ProtocolErrorKind::RecordCountHeaderInvalid)),
            },
            Some(Err(_)) => return Err(invalid(ProtocolErrorKind::RecordCountHeaderInvalid)),
        };
        let rows = match reader.open::<T>(incoming) {
            Ok(rows) => rows,
            Err(error) => return Err(SfError::from_open(error).await),
        };
        Ok(ResultPartRows {
            rows,
            operation,
            next,
            declared_records,
            end: PartEnd::Reading,
        })
    }
}

/// How far a [`ResultPartRows`] got.
#[derive(Clone, Copy)]
enum PartEnd {
    /// Rows may follow.
    Reading,
    /// A read error, a limit or a record-count mismatch ended the part (returned by `next`).
    Failed,
    /// The parsed end of input, validated.
    Complete(PartComplete),
}

/// Evidence that one result part was read to its **validated** end: the CSV reader reached the
/// end of input (not a limit, not a read error, not an early stop) and the parsed count equals
/// `Sforce-NumberOfRecords` when the response declared one. Only [`ResultPartRows::finish`]
/// produces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PartComplete {
    /// Rows parsed and delivered.
    pub records: u64,
    /// CSV bytes consumed.
    pub bytes: u64,
    /// `Sforce-NumberOfRecords`, when present (validated against `records`).
    pub declared_records: Option<u64>,
}

/// One open result part: typed rows read incrementally, its response metadata, and its validated
/// end ([`finish`](Self::finish)).
///
/// Nothing runs ahead of the caller (one record framed at a time, see
/// [`concord::csv::CsvRecords`]). Dropping it drops the response without reading the rest.
/// `Debug` shows the operation, the counts and the state, never a locator or a row.
pub struct ResultPartRows<T> {
    rows: CsvRecords<T>,
    operation: &'static str,
    next: NextResultSet,
    declared_records: Option<u64>,
    end: PartEnd,
}

impl<T> ResultPartRows<T> {
    /// The next row; `None` at the validated end of the part, and after an error.
    ///
    /// A failure ends the part: [`SfError::Read`] (transport, format, decode, or a `Limit` such
    /// as `maxRecords`), or, at the parsed end of input, a [`ProtocolError`] of kind
    /// [`RecordCountMismatch`](ProtocolErrorKind::RecordCountMismatch) when the response declared
    /// another count. Rows already returned stay returned; they are not complete until
    /// [`finish`](Self::finish) says so.
    pub async fn next(&mut self) -> Option<Result<T, SfError>> {
        if !matches!(self.end, PartEnd::Reading) {
            return None;
        }
        match self.rows.next().await {
            Some(Ok(row)) => Some(Ok(row)),
            Some(Err(error)) => {
                // Every CSV read error is terminal.
                self.end = PartEnd::Failed;
                Some(Err(error.into()))
            }
            None => {
                let position = self.rows.position();
                if self.rows.completion() != Completion::EndOfInput {
                    // Not reachable after the errors above; never treated as an end of input.
                    self.end = PartEnd::Failed;
                    return None;
                }
                if let Some(declared) = self.declared_records
                    && declared != position.records
                {
                    self.end = PartEnd::Failed;
                    return Some(Err(ProtocolError::new(
                        self.operation,
                        ProtocolErrorKind::RecordCountMismatch {
                            declared,
                            parsed: position.records,
                        },
                    )
                    .into()));
                }
                self.end = PartEnd::Complete(PartComplete {
                    records: position.records,
                    bytes: position.bytes,
                    declared_records: self.declared_records,
                });
                None
            }
        }
    }

    /// The part's completion, **only** after [`next`](Self::next) returned `None` at its
    /// validated end.
    ///
    /// # Errors
    ///
    /// [`SfError::Incomplete`] when the part was not read to that end (the caller stopped
    /// early) or a read, limit or record-count error ended it (that error was returned by
    /// `next`).
    pub fn finish(self) -> Result<PartComplete, SfError> {
        match self.end {
            PartEnd::Complete(complete) => Ok(complete),
            PartEnd::Reading | PartEnd::Failed => Err(IncompletePart::new(
                self.operation,
                matches!(self.end, PartEnd::Failed),
                self.rows.position(),
            )
            .into()),
        }
    }

    /// What `Sforce-Locator` said about the next set. The sequential chain requires it
    /// ([`ResultSets`]); a parallel result URL's response need not carry it
    /// ([`NextResultSet::Unknown`]).
    pub fn next_set(&self) -> &NextResultSet {
        &self.next
    }

    /// `Sforce-NumberOfRecords`, when present.
    pub fn declared_records(&self) -> Option<u64> {
        self.declared_records
    }

    /// Rows delivered and CSV bytes consumed so far.
    pub fn position(&self) -> Position {
        self.rows.position()
    }

    /// The declared endpoint that produced the part (named by its errors).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// The underlying CSV reader, for inspection (its header, retained buffer sizes). Rows are
    /// read only through [`next`](Self::next), so the end is always validated.
    pub fn records(&self) -> &CsvRecords<T> {
        &self.rows
    }
}

impl<T> fmt::Debug for ResultPartRows<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self.end {
            PartEnd::Reading => "Reading",
            PartEnd::Failed => "Failed",
            PartEnd::Complete(_) => "Complete",
        };
        f.debug_struct("ResultPartRows")
            .field("operation", &self.operation)
            .field("next", &self.next)
            .field("declared_records", &self.declared_records)
            .field("position", &self.rows.position())
            .field("state", &state)
            .finish()
    }
}

/// A discovered result URL: its discovery ordinal (0-based, stable across resumed discovery) and
/// the validated link. `Debug` never shows the locator.
#[derive(Clone, PartialEq, Eq, Hash, concord::Debug)]
pub struct ResultPart {
    /// Position in discovery order.
    pub ordinal: u64,
    /// The validated link.
    pub link: ResultLink,
}

/// The key of a result-pages manifest after the first: the next link's locator and the ordinal
/// its first part gets.
#[derive(Clone, PartialEq, Eq, Hash, concord::Debug)]
pub struct ManifestCursor {
    /// The manifest's `locator`.
    #[sensitive]
    pub locator: ResultLocator,
    /// The ordinal of its first descriptor.
    pub next_ordinal: u64,
}

/// The discovery factory of one job's parallel result pages.
#[derive(Clone, Debug)]
pub struct ResultManifests {
    job: BulkQueryJob,
}

impl ResultManifests {
    /// The job.
    pub fn job(&self) -> &BulkQueryJob {
        &self.job
    }

    /// The manifest at `key` (`None`: the first). The future owns its data.
    pub fn fetch(
        &self,
        key: Option<ManifestCursor>,
    ) -> impl Future<Output = Result<Manifest<ResultPart, ManifestCursor>, SfError>> + 'static {
        let job = self.job.clone();
        async move {
            let (locator, first_ordinal) = match key {
                None => (None, 0),
                Some(cursor) => (Some(cursor.locator), cursor.next_ordinal),
            };
            let locator_requested = locator.clone();
            let op = job
                .ctx
                .api()
                .bulk_query()
                .job(job.id.clone())
                .result_pages()
                .maybe_locator(locator);
            let manifest = send(prepared(op.prepare())?).await?;
            let next_link = manifest.next_link();
            let mut parts = Vec::with_capacity(manifest.result_pages.len());
            for (i, entry) in manifest.result_pages.iter().enumerate() {
                let link = job.ctx.result_link(&job.id, &entry.result_url)?;
                parts.push(ResultPart {
                    ordinal: first_ordinal + i as u64,
                    link,
                });
            }
            let next_ordinal = first_ordinal + parts.len() as u64;
            let next = match (manifest.done, next_link) {
                (_, NextLink::Ambiguous) => {
                    return Err(
                        ProtocolError::new(MANIFEST, ProtocolErrorKind::AmbiguousNextLink).into(),
                    );
                }
                (true, NextLink::Absent) => None,
                (true, NextLink::Present(_)) => {
                    return Err(
                        ProtocolError::new(MANIFEST, ProtocolErrorKind::NextLinkWhenDone).into(),
                    );
                }
                (false, NextLink::Absent) => {
                    return Err(
                        ProtocolError::new(MANIFEST, ProtocolErrorKind::MissingNextLink).into(),
                    );
                }
                (false, NextLink::Present(raw)) => {
                    let locator = job.ctx.result_pages_link(&job.id, raw)?;
                    if locator_requested.as_ref() == Some(&locator) {
                        return Err(ProtocolError::new(
                            MANIFEST,
                            ProtocolErrorKind::LocatorDidNotAdvance,
                        )
                        .into());
                    }
                    Some(ManifestCursor {
                        locator,
                        next_ordinal,
                    })
                }
            };
            Ok(Manifest::new(parts, next))
        }
    }
}

/// Metadata of one result set read through the sequential locator chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultSetMeta {
    /// `Sforce-NumberOfRecords`.
    pub declared_records: Option<u64>,
    /// Input bytes the CSV reader consumed.
    pub bytes: u64,
}

/// One result set as a pagination page; the key is the header-derived `Sforce-Locator`.
pub type ResultSetPage<T> = Page<T, ResultLocator, ResultSetMeta>;

/// The page factory of a job's result sets as a dependent chain: each set's `Sforce-Locator`
/// header is the key of the next (`"null"` on the last; a missing header is a
/// [`ProtocolErrorKind::LocatorHeaderMissing`], the locator just requested a
/// [`ProtocolErrorKind::LocatorDidNotAdvance`]). Each set is read by the job's
/// [`ResultPartReader`]: the job's dialect, a header that is exactly the selected fields, at most
/// `maxRecords` rows ([`concord::stream::ReadErrorKind::Limit`] beyond), and a parsed count equal
/// to `Sforce-NumberOfRecords` when declared. A page is returned only after the set's validated
/// end ([`ResultPartRows::finish`]).
///
/// The parallel route ([`ResultManifests`]) is the primary export path; this chain reads the same
/// results one set at a time, collected per set.
pub struct ResultSets<T> {
    parts: ResultPartReader<T>,
    max_records: Option<MaxRecords>,
}

impl<T: DeserializeOwned + 'static> ResultSets<T> {
    /// The chain of `job`'s result sets with rows typed `T`, whose header must be `columns`.
    ///
    /// # Errors
    ///
    /// [`SfError::Csv`] for a refused dialect.
    pub fn new(
        job: &BulkQueryJob,
        columns: &[FieldPath],
        max_records: Option<MaxRecords>,
    ) -> Result<Self, SfError> {
        Ok(Self {
            parts: ResultPartReader::new(job, columns)?,
            max_records,
        })
    }

    /// The result set at `key` (`None`: the first).
    pub fn fetch(
        &self,
        key: Option<ResultLocator>,
    ) -> impl Future<Output = Result<ResultSetPage<T>, SfError>> + 'static {
        let parts = self.parts.clone();
        let link = ResultLink {
            locator: key,
            max_records: self.max_records,
        };
        async move {
            let mut set = parts.open(&link).await?;
            // The sequential chain's locator rules (the parallel descriptors do not need them).
            let next = match set.next_set() {
                NextResultSet::Last => None,
                NextResultSet::At(locator) if link.locator.as_ref() == Some(locator) => {
                    return Err(ProtocolError::new(
                        set.operation(),
                        ProtocolErrorKind::LocatorDidNotAdvance,
                    )
                    .into());
                }
                NextResultSet::At(locator) => Some(locator.clone()),
                NextResultSet::Unknown => {
                    return Err(ProtocolError::new(
                        set.operation(),
                        ProtocolErrorKind::LocatorHeaderMissing,
                    )
                    .into());
                }
            };
            let mut items = Vec::new();
            while let Some(row) = set.next().await {
                items.push(row?);
            }
            let complete = set.finish()?;
            Ok(Page {
                items,
                next,
                meta: ResultSetMeta {
                    declared_records: complete.declared_records,
                    bytes: complete.bytes,
                },
            })
        }
    }
}
