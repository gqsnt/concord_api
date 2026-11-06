//! The incremental reader: [`CsvReader`] and its [`CsvRecords`].

use std::fmt;
use std::sync::Arc;

use concord_core::{CodecError, ResponseHead, reqwest};
use serde::de::DeserializeOwned;

use super::codec::check_media;
use super::frame::{Mode, Step, Table};
use super::options::{Config, CsvConfigError, CsvOptions};
use super::record::{self, MapFn, RawRecord};
use crate::stream::{
    ByteSource, Driver, OpenError, Openable, Position, ReadError, SKIP_BUDGET, open_input,
    open_response, yield_now,
};

/// An immutable, validated CSV reader configuration: opens responses and byte sources into
/// [`CsvRecords`].
///
/// From a static type (`Csv::<HasHeader, Semicolon, ProviderDialect>::reader()`) or from runtime
/// options ([`CsvReader::new`]); both resolve to the same thing, so equal options read equally.
///
/// ```
/// use concord_extras::csv::{CsvOptions, CsvReader, HeaderMode, ReadMapping};
/// use concord_extras::stream::Chunks;
///
/// #[derive(serde::Deserialize, Debug, PartialEq)]
/// struct Row { id: u32, name: String }
///
/// # tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
/// let selected = b';'; // chosen per job
/// let reader = CsvReader::new(
///     CsvOptions::new(HeaderMode::Absent, selected).read_mapping(ReadMapping::columns(["id", "name"])),
/// )?;
/// let mut rows = reader.read::<Row, _>(Chunks::new(["1;Al", "ice\n2;Bob\n"]));
/// assert_eq!(rows.next().await.transpose()?, Some(Row { id: 1, name: "Alice".into() }));
/// assert_eq!(rows.next().await.transpose()?, Some(Row { id: 2, name: "Bob".into() }));
/// assert!(rows.next().await.is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(()) }).unwrap();
/// ```
#[derive(Clone)]
pub struct CsvReader {
    config: Arc<Config>,
}

impl CsvReader {
    /// A reader of these options.
    ///
    /// # Errors
    ///
    /// A [`CsvConfigError`] for an invalid dialect or column list. Nothing is read.
    pub fn new(options: CsvOptions) -> Result<Self, CsvConfigError> {
        Ok(Self {
            config: Arc::new(Config::new(options)?),
        })
    }

    /// The options.
    pub fn options(&self) -> &CsvOptions {
        &self.config.options
    }

    pub(crate) fn from_config(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Typed rows of a body whose head the static format already checked (`Incremental`).
    pub(crate) fn open_checked<T: DeserializeOwned>(
        &self,
        body: concord_core::Incoming,
    ) -> CsvRecords<T> {
        let (response, operation) = body.into_parts();
        self.records(
            response,
            operation.map(|op| op.name()),
            record::typed::<T>,
            Mode::Typed,
        )
    }

    /// Raw records of a body whose head the static format already checked (`Incremental`).
    pub(crate) fn open_raw_checked(&self, body: concord_core::Incoming) -> CsvRecords<RawRecord> {
        let (response, operation) = body.into_parts();
        self.records(
            response,
            operation.map(|op| op.name()),
            record::raw,
            Mode::Raw,
        )
    }

    /// The `Content-Type` this reader accepts, as checked by [`open`](Self::open) (per the
    /// [`MediaPolicy`](super::MediaPolicy)), for a caller that opens responses itself.
    ///
    /// # Errors
    ///
    /// The refusal, with a static label.
    pub fn check_head(&self, head: &ResponseHead<'_>) -> Result<(), CodecError> {
        check_media(head, self.config.has_header(), self.config.media())
    }

    /// Opens a response for typed rows: the [`Incoming`](concord_core::Incoming) body of an
    /// endpoint declared `-> stream`, a [`Received`](concord_core::Received) response or a native
    /// `reqwest::Response` ([`Openable`]). Before polling the body: requires a 2xx status, then
    /// the head per [`check_head`](Self::check_head). A declared endpoint's identity names every
    /// later error.
    ///
    /// # Errors
    ///
    /// An [`OpenError`] that keeps the response unread.
    pub fn open<T: DeserializeOwned>(
        &self,
        response: impl Openable,
    ) -> Result<CsvRecords<T>, OpenError> {
        let (response, operation) = open_input(response, |head| self.check_head(head))?;
        Ok(self.records(response, operation, record::typed::<T>, Mode::Typed))
    }

    /// [`open`](Self::open), naming the operation in every later error (a static label the SDK
    /// chooses; never derived from the URL).
    ///
    /// # Errors
    ///
    /// An [`OpenError`] that keeps the response unread.
    pub fn open_for<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        operation: &'static str,
    ) -> Result<CsvRecords<T>, OpenError> {
        self.opened(response, Some(operation), record::typed::<T>, Mode::Typed)
    }

    /// Typed rows from any byte source, without status or head checks.
    pub fn read<T: DeserializeOwned, S: ByteSource>(&self, source: S) -> CsvRecords<T, S> {
        self.records(source, None, record::typed::<T>, Mode::Typed)
    }

    /// Opens a response ([`Openable`]) for raw byte records (no UTF-8 requirement; header names
    /// may be empty or repeated). The head checks are those of [`open`](Self::open).
    ///
    /// # Errors
    ///
    /// An [`OpenError`] that keeps the response unread.
    pub fn open_raw(&self, response: impl Openable) -> Result<CsvRecords<RawRecord>, OpenError> {
        let (response, operation) = open_input(response, |head| self.check_head(head))?;
        Ok(self.records(response, operation, record::raw, Mode::Raw))
    }

    /// [`open_raw`](Self::open_raw), naming the operation in every later error.
    ///
    /// # Errors
    ///
    /// An [`OpenError`] that keeps the response unread.
    pub fn open_raw_for(
        &self,
        response: reqwest::Response,
        operation: &'static str,
    ) -> Result<CsvRecords<RawRecord>, OpenError> {
        self.opened(response, Some(operation), record::raw, Mode::Raw)
    }

    /// Raw byte records from any byte source, without status or head checks.
    pub fn read_raw<S: ByteSource>(&self, source: S) -> CsvRecords<RawRecord, S> {
        self.records(source, None, record::raw, Mode::Raw)
    }

    fn opened<T>(
        &self,
        response: reqwest::Response,
        operation: Option<&'static str>,
        map: MapFn<T>,
        mode: Mode,
    ) -> Result<CsvRecords<T>, OpenError> {
        let response = open_response(response, operation, |head| self.check_head(head))?;
        Ok(self.records(response, operation, map, mode))
    }

    fn records<T, S: ByteSource>(
        &self,
        source: S,
        operation: Option<&'static str>,
        map: MapFn<T>,
        mode: Mode,
    ) -> CsvRecords<T, S> {
        CsvRecords {
            driver: Driver::new(source, operation)
                .with_totals(self.config.max_total_bytes(), self.config.max_records()),
            table: Table::new(Arc::clone(&self.config), mode),
            map,
        }
    }
}

impl fmt::Debug for CsvReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CsvReader")
            .field("options", &self.config.options)
            .finish()
    }
}

/// Rows read incrementally from one response or byte source: `T` (a `DeserializeOwned` row) or
/// [`RawRecord`].
///
/// Each [`next`](Self::next) reads only as much input as the next record needs (plus the rest of
/// the current chunk): nothing runs ahead of the caller, no task is spawned. The record being
/// framed is bounded by the record/header/field limits; a long run of skipped input (comment or
/// blank lines) or a large record yields to the executor after every 64 KiB parsed, and a run of
/// empty chunks from the source after every 64 chunks. After an error or the end, `next` returns
/// `None`.
/// Dropping it drops the source (the response's connection).
pub struct CsvRecords<T, S = reqwest::Response> {
    driver: Driver<S>,
    table: Table,
    map: MapFn<T>,
}

impl<T, S: ByteSource> CsvRecords<T, S> {
    /// Whether the input ended cleanly ([`Completion::EndOfInput`](crate::stream::Completion)),
    /// a terminal error ended the reader (`Failed`), or records may follow (`Reading`). Only
    /// `EndOfInput` proves every record was read; stopping early or dropping the reader is not
    /// an end of input.
    pub fn completion(&self) -> crate::stream::Completion {
        self.driver.completion()
    }

    /// The next row; `None` at the end of the table, and after an error.
    ///
    /// A failure is a [`ReadError`]: [`Transport`](crate::stream::ReadErrorKind::Transport) (the
    /// source failed), [`Format`](crate::stream::ReadErrorKind::Format) (width, header names,
    /// UTF-8, a refused BOM, a missing header), [`Decode`](crate::stream::ReadErrorKind::Decode)
    /// (a well-formed record that is not a `T`) or [`Limit`](crate::stream::ReadErrorKind::Limit).
    /// Rows already returned stay returned.
    pub async fn next(&mut self) -> Option<Result<T, ReadError>> {
        if self.driver.is_fused() {
            return None;
        }
        let mut work = 0usize;
        loop {
            if self.driver.input().is_empty()
                && !self.driver.at_eof()
                && let Err(e) = self.driver.refill().await
            {
                return Some(Err(e));
            }
            if work >= SKIP_BUDGET {
                yield_now().await;
                work = 0;
            }
            // At most `SKIP_BUDGET` input bytes are parsed between two yields.
            let input = self.driver.input();
            let input = &input[..input.len().min(SKIP_BUDGET - work)];
            match self.table.step(input) {
                Ok((n, Step::Progress)) => {
                    self.driver.consume(n);
                    work += n;
                }
                Ok((n, Step::Record(record))) => {
                    self.driver.consume(n);
                    if let Err(e) = self.driver.check_record_total() {
                        return Some(Err(e));
                    }
                    return Some(match (self.map)(&self.table, record) {
                        Ok(row) => {
                            self.driver.delivered();
                            Ok(row)
                        }
                        Err(e) => Err(self.driver.error(e.kind, e.codec)),
                    });
                }
                Ok((n, Step::End)) => {
                    self.driver.consume(n);
                    self.driver.finish();
                    return None;
                }
                Err(e) => return Some(Err(self.driver.error(e.kind, e.codec))),
            }
        }
    }

    /// Records delivered and input bytes consumed so far.
    pub fn position(&self) -> Position {
        self.driver.position()
    }

    /// Library-owned bytes currently allocated for the record being framed: the buffer of its
    /// unescaped field bytes plus the buffer of its field end offsets (`usize` each). The
    /// buffers are reused from record to record, so this is their current capacity, not the size
    /// of the last record; it never exceeds `max(max_header_bytes, max_record_bytes) +
    /// max_fields * size_of::<usize>()`.
    ///
    /// Not counted: the unconsumed rest of the current transport chunk (the source's memory, see
    /// [`pending_input_bytes`](Self::pending_input_bytes)), the kept header record and its names
    /// ([`headers`](Self::headers)), the fixed-size parser state, the rows already returned (the
    /// caller's) and the memory of the transport (reqwest/hyper buffers).
    pub fn retained_bytes(&self) -> usize {
        self.table.retained()
    }

    /// The largest [`retained_bytes`](Self::retained_bytes) value over this reader's life.
    pub fn peak_retained_bytes(&self) -> usize {
        self.table.peak_retained()
    }

    /// The bytes of the current transport chunk not yet consumed by the parser (a shared slice
    /// of the chunk the source returned; at most one chunk).
    pub fn pending_input_bytes(&self) -> usize {
        self.driver.input().len()
    }

    /// The header record, once it has been read (after the first `next()` of a `HasHeader`
    /// table); `None` for a headerless table.
    pub fn headers(&self) -> Option<&RawRecord> {
        self.table.header()
    }

    /// The operation name supplied when the response was opened, if any.
    pub fn operation(&self) -> Option<&'static str> {
        self.driver.operation()
    }

    /// The byte source (the partly read response), for a caller that deliberately needs it
    /// after a failure. Input already consumed is gone.
    pub fn into_source(self) -> S {
        self.driver.into_source()
    }
}

impl<T, S> fmt::Debug for CsvRecords<T, S>
where
    S: ByteSource,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CsvRecords")
            .field("operation", &self.driver.operation())
            .field("position", &self.driver.position())
            .field("finished", &self.driver.is_fused())
            .finish_non_exhaustive()
    }
}

impl<T, S> crate::stream::RecordSealed for CsvRecords<T, S> {}

impl<T, S> crate::stream::RecordReader for CsvRecords<T, S>
where
    T: Send + 'static,
    S: ByteSource + 'static,
{
    type Record = T;

    fn next_record(
        &mut self,
    ) -> impl std::future::Future<Output = Option<Result<T, ReadError>>> + Send {
        self.next()
    }

    fn record_position(&self) -> Position {
        self.position()
    }

    fn record_operation(&self) -> Option<&'static str> {
        self.driver.operation()
    }

    fn record_completion(&self) -> crate::stream::Completion {
        self.driver.completion()
    }
}

impl<T, S> CsvRecords<T, S>
where
    T: Send + 'static,
    S: ByteSource + 'static,
{
    /// This reader as an owning standard [`Stream`](futures_core::Stream) of
    /// `Result<T, ReadError>` ([`RecordStream`](crate::stream::RecordStream)): no read-ahead,
    /// cancellation-safe, errors and completion unchanged.
    pub fn into_stream(self) -> crate::stream::RecordStream<Self> {
        crate::stream::RecordStream::new(self)
    }
}
