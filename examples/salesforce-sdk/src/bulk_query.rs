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

use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::num::NonZeroUsize;

use concord::csv::CsvReader;
use concord::reqwest;
use concord_pagination::{Manifest, Page};
use serde::de::DeserializeOwned;

use crate::bulk::{BulkDialect, CreateQueryJob, JobState, JobStateChange, NextLink, QueryJobInfo};
use crate::context::{OrgContext, ResultLink};
use crate::csv::SalesforceCsv;
use crate::error::{JobFailure, ProtocolError, ProtocolErrorKind, SfError};
use crate::ids::{MaxRecords, QueryJobId, ResultLocator};
use crate::soql::{FieldPath, Soql};
use crate::transport::{prepared, send};

const CREATE: &str = "bulk_query.create";
const INFO: &str = "bulk_query.job.info";
const MANIFEST: &str = "bulk_query.job.result_pages";
/// The operation label of result-set downloads (CSV readers name it in their errors).
pub const RESULTS: &str = "bulk_query.job.results";

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

    /// Requests one result set (`GET jobs/query/{id}/results[?locator=..][&maxRecords=..]`) and
    /// reads its headers; the CSV body is left unread for a reader.
    ///
    /// # Errors
    ///
    /// [`SfError::Api`] for a non-2xx status (its error document read), a transport error, or a
    /// [`ProtocolError`] for a malformed `Sforce-Locator` / `Sforce-NumberOfRecords`.
    pub async fn open_result_set(&self, link: &ResultLink) -> Result<ResultSet, SfError> {
        let op = self
            .ctx
            .api()
            .bulk_query()
            .job(self.id.clone())
            .results()
            .maybe_locator(link.locator.clone())
            .maybe_max_records(link.max_records);
        let response = match prepared(op.prepare())?.send_raw().await {
            Ok(response) => response,
            Err(error) => return Err(SfError::from_concord(error).await),
        };
        if !response.status().is_success() {
            // Let the CSV opener's status refusal read the error document.
            return Ok(ResultSet {
                response,
                next: NextResultSet::Unknown,
                declared_records: None,
            });
        }
        let headers = response.headers();
        let next = match headers.get("sforce-locator").map(|v| v.to_str()) {
            None => NextResultSet::Unknown,
            Some(Ok("null")) => NextResultSet::Last,
            Some(Ok(text)) => match ResultLocator::new(text) {
                Ok(locator) => NextResultSet::At(locator),
                Err(_) => {
                    return Err(ProtocolError::new(
                        RESULTS,
                        ProtocolErrorKind::LocatorHeaderInvalid,
                    )
                    .into());
                }
            },
            Some(Err(_)) => {
                return Err(
                    ProtocolError::new(RESULTS, ProtocolErrorKind::LocatorHeaderInvalid).into(),
                );
            }
        };
        let declared_records = match headers.get("sforce-numberofrecords").map(|v| v.to_str()) {
            None => None,
            Some(Ok(text)) => Some(text.trim().parse::<u64>().map_err(|_| {
                SfError::from(ProtocolError::new(
                    RESULTS,
                    ProtocolErrorKind::RecordCountHeaderInvalid,
                ))
            })?),
            Some(Err(_)) => {
                return Err(ProtocolError::new(
                    RESULTS,
                    ProtocolErrorKind::RecordCountHeaderInvalid,
                )
                .into());
            }
        };
        Ok(ResultSet {
            response,
            next,
            declared_records,
        })
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
#[derive(Clone, PartialEq, Eq)]
pub enum NextResultSet {
    /// `Sforce-Locator: null`: this is the last set.
    Last,
    /// The next set's locator.
    At(ResultLocator),
    /// No header (a parallel result URL's response need not be read for it).
    Unknown,
}

impl fmt::Debug for NextResultSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NextResultSet::Last => "Last",
            NextResultSet::At(_) => "At(..)",
            NextResultSet::Unknown => "Unknown",
        })
    }
}

/// One result set's response, headers read, CSV body unread.
pub struct ResultSet {
    /// The response (its status may be non-2xx: the CSV opener reads the error document).
    pub response: reqwest::Response,
    /// `Sforce-Locator`.
    pub next: NextResultSet,
    /// `Sforce-NumberOfRecords`, when present.
    pub declared_records: Option<u64>,
}

impl fmt::Debug for ResultSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultSet")
            .field("status", &self.response.status().as_u16())
            .field("next", &self.next)
            .field("declared_records", &self.declared_records)
            .finish()
    }
}

/// A discovered result URL: its discovery ordinal (0-based, stable across resumed discovery) and
/// the validated link. `Debug` never shows the locator.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ResultPart {
    /// Position in discovery order.
    pub ordinal: u64,
    /// The validated link.
    pub link: ResultLink,
}

impl fmt::Debug for ResultPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultPart")
            .field("ordinal", &self.ordinal)
            .field("link", &self.link)
            .finish()
    }
}

/// The key of a result-pages manifest after the first: the next link's locator and the ordinal
/// its first part gets.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ManifestCursor {
    /// The manifest's `locator`.
    pub locator: ResultLocator,
    /// The ordinal of its first descriptor.
    pub next_ordinal: u64,
}

impl fmt::Debug for ManifestCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManifestCursor")
            .field("next_ordinal", &self.next_ordinal)
            .finish_non_exhaustive()
    }
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
/// header is the key of the next (`"null"` on the last; the locator just requested is a
/// [`ProtocolErrorKind::LocatorDidNotAdvance`]). Each page's rows are parsed with the
/// job's dialect, the header must be exactly the selected fields, and the parsed count must equal
/// `Sforce-NumberOfRecords`. A page holds at most `maxRecords` rows: the reader refuses more
/// ([`concord::stream::ReadErrorKind::Limit`]).
///
/// The parallel route ([`ResultManifests`]) is the primary export path; this chain reads the same
/// results one set at a time.
pub struct ResultSets<T> {
    job: BulkQueryJob,
    reader: CsvReader,
    max_records: Option<MaxRecords>,
    _rows: PhantomData<fn() -> T>,
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
        let csv = SalesforceCsv::new(job.dialect());
        let names: Vec<String> = columns.iter().map(ToString::to_string).collect();
        let mut options = csv.options().columns(names);
        if let Some(max) = max_records {
            options = options.max_records(u64::from(max.get()));
        }
        let reader = CsvReader::new(options).map_err(crate::csv::CsvError::from)?;
        Ok(Self {
            job: job.clone(),
            reader,
            max_records,
            _rows: PhantomData,
        })
    }

    /// The result set at `key` (`None`: the first).
    pub fn fetch(
        &self,
        key: Option<ResultLocator>,
    ) -> impl Future<Output = Result<ResultSetPage<T>, SfError>> + 'static {
        let job = self.job.clone();
        let reader = self.reader.clone();
        let link = ResultLink {
            locator: key,
            max_records: self.max_records,
        };
        async move {
            let set = job.open_result_set(&link).await?;
            let declared_records = set.declared_records;
            let next = set.next.clone();
            let mut rows = match reader.open_for::<T>(set.response, RESULTS) {
                Ok(rows) => rows,
                Err(error) => return Err(SfError::from_open(error).await),
            };
            let next = match next {
                NextResultSet::Last => None,
                NextResultSet::At(locator) if link.locator.as_ref() == Some(&locator) => {
                    return Err(ProtocolError::new(
                        RESULTS,
                        ProtocolErrorKind::LocatorDidNotAdvance,
                    )
                    .into());
                }
                NextResultSet::At(locator) => Some(locator),
                NextResultSet::Unknown => {
                    return Err(ProtocolError::new(
                        RESULTS,
                        ProtocolErrorKind::LocatorHeaderMissing,
                    )
                    .into());
                }
            };
            let mut items = Vec::new();
            while let Some(row) = rows.next().await {
                items.push(row?);
            }
            if let Some(declared) = declared_records
                && declared != items.len() as u64
            {
                return Err(ProtocolError::new(
                    RESULTS,
                    ProtocolErrorKind::RecordCountMismatch {
                        declared,
                        parsed: items.len() as u64,
                    },
                )
                .into());
            }
            Ok(Page {
                items,
                next,
                meta: ResultSetMeta {
                    declared_records,
                    bytes: rows.position().bytes,
                },
            })
        }
    }
}
