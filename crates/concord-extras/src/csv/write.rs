//! Writing: [`Rows`], [`StreamRows`], [`CsvWriter`], and the one encoder behind the finite and
//! lazy routes.

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use concord_core::CodecError;
use concord_core::reqwest::header::HeaderValue;
use futures_core::Stream;
use serde::Serialize;

use super::names::{Shape, shape};
use super::options::{Config, CsvConfigError, CsvOptions, check_columns};
use super::record::RawRecord;
use crate::stream::{RecordBody, RecordEncoder, Upload};

/// A finite table to write: an iterator of rows and, optionally, explicit columns.
///
/// Encoded before sending ([`Encode<Rows<I>>`](concord_core::Encode),
/// [`CsvWriter::to_vec`]): an iterator does not make the HTTP body streaming (use
/// [`StreamRows`] for that).
///
/// ```
/// use concord_extras::csv::{Csv, Rows};
///
/// let writer = Csv::<concord_extras::csv::HasHeader>::writer().unwrap();
/// let body = writer.to_vec(Rows::new([(1, "Lyon"), (2, "Zoë")]).with_columns(["id", "name"])).unwrap();
/// assert_eq!(body, "id,name\r\n1,Lyon\r\n2,Zoë\r\n".as_bytes());
/// ```
pub struct Rows<I> {
    rows: I,
    columns: Option<Vec<String>>,
}

impl<I> Rows<I> {
    /// The rows, without explicit columns.
    pub fn new(rows: I) -> Self {
        Self {
            rows,
            columns: None,
        }
    }

    /// Explicit columns: the header of a `HasHeader` table (required for rows without field
    /// names, and for an empty table), and the expected field names, in order, of named rows.
    /// A profile with different columns refuses them.
    #[must_use]
    pub fn with_columns<C, S>(mut self, columns: C) -> Self
    where
        C: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.columns = Some(columns.into_iter().map(Into::into).collect());
        self
    }
}

impl<I> fmt::Debug for Rows<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rows")
            .field("columns", &self.columns.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

/// A one-shot source of rows for a lazily produced body: a [`Stream`] of `Result<T, E>`, and,
/// optionally, explicit columns (as [`Rows::with_columns`]).
///
/// Preparing the request ([`Encode<StreamRows<S>>`](concord_core::Encode),
/// [`CsvWriter::upload`]) polls it zero times; sending polls it as the transport asks, once. A
/// row error, a changed schema, an empty header-required source without columns or a source
/// error fails the transfer after earlier bytes were sent; nothing is replayed or rolled back.
/// [`stream::from_iter`](crate::stream::from_iter) makes a source of an iterator.
pub struct StreamRows<S> {
    rows: S,
    columns: Option<Vec<String>>,
}

impl<S> StreamRows<S> {
    /// The source, without explicit columns.
    pub fn new(rows: S) -> Self {
        Self {
            rows,
            columns: None,
        }
    }

    /// Explicit columns, as [`Rows::with_columns`]. A `HasHeader` source needs them to write
    /// its header before the first row arrives, and to end empty.
    #[must_use]
    pub fn with_columns<C, N>(mut self, columns: C) -> Self
    where
        C: IntoIterator<Item = N>,
        N: Into<String>,
    {
        self.columns = Some(columns.into_iter().map(Into::into).collect());
        self
    }
}

impl<S> fmt::Debug for StreamRows<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamRows")
            .field("columns", &self.columns.as_ref().map(Vec::len))
            .finish_non_exhaustive()
    }
}

/// An immutable, validated CSV writer configuration. From a static type
/// (`Csv::<HasHeader, Semicolon>::writer()`) or from runtime options ([`CsvWriter::new`]).
///
/// A runtime-selected header mode changes the `Content-Type` (`header=present`/`absent`), which a
/// static [`Encode`](concord_core::Encode) cannot express: [`upload`](Self::upload) returns the
/// body together with its `Content-Type` ([`Upload`]), for the SDK to apply to the native
/// request.
#[derive(Clone)]
pub struct CsvWriter {
    config: Arc<Config>,
}

impl CsvWriter {
    /// A writer of these options.
    ///
    /// # Errors
    ///
    /// A [`CsvConfigError`] for an invalid dialect or column list.
    pub fn new(options: CsvOptions) -> Result<Self, CsvConfigError> {
        Ok(Self {
            config: Arc::new(Config::new(options)?),
        })
    }

    /// The options.
    pub fn options(&self) -> &CsvOptions {
        &self.config.options
    }

    /// The `Content-Type` of this writer's bodies: `text/csv; header=present` or
    /// `text/csv; header=absent`.
    pub fn content_type(&self) -> HeaderValue {
        HeaderValue::from_static(self.config.content_type())
    }

    /// Encodes a finite table into bytes.
    ///
    /// # Errors
    ///
    /// A [`CodecError`]: conflicting or invalid columns, rows that cannot form the table (see
    /// the [module documentation](super#writing)), a row that does not serialize, a limit.
    pub fn to_vec<I>(&self, rows: Rows<I>) -> Result<Vec<u8>, CodecError>
    where
        I: IntoIterator,
        I::Item: Serialize,
    {
        encode_rows(Arc::clone(&self.config), rows)
    }

    /// A lazily produced body over a one-shot source, with its `Content-Type`. Nothing is polled
    /// here; see [`StreamRows`] for the late failures.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] when the source's explicit columns are invalid or conflict with the
    /// configured columns (found before any polling).
    pub fn upload<S, T, E>(&self, rows: StreamRows<S>) -> Result<Upload, CodecError>
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        T: Serialize + 'static,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        let body = stream_body(Arc::clone(&self.config), rows)?;
        Ok(Upload::new(body, self.content_type()))
    }
}

impl fmt::Debug for CsvWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CsvWriter")
            .field("options", &self.config.options)
            .finish()
    }
}

pub(crate) fn encode_rows<I>(config: Arc<Config>, rows: Rows<I>) -> Result<Vec<u8>, CodecError>
where
    I: IntoIterator,
    I::Item: Serialize,
{
    let mut encoder = Encoder::new(config, rows.columns)?;
    let mut out = Vec::new();
    encoder.begin(&mut out)?;
    for row in rows.rows {
        encoder.row(&row, &mut out)?;
    }
    encoder.end(&mut out)?;
    Ok(out)
}

pub(crate) fn encode_raw(
    config: Arc<Config>,
    table: super::RawTable,
) -> Result<Vec<u8>, CodecError> {
    let mut encoder = Encoder::new(config, None)?;
    let mut out = Vec::new();
    match (encoder.has_header(), table.headers) {
        (true, Some(header)) => encoder.raw_header(&header, &mut out)?,
        (true, None) => {
            return Err(CodecError::new(
                "a CSV table with a header needs its header record",
            ));
        }
        (false, Some(_)) => {
            return Err(CodecError::new(
                "a headerless CSV table cannot carry a header record",
            ));
        }
        (false, None) => {}
    }
    for record in &table.records {
        encoder.row(record, &mut out)?;
    }
    encoder.end(&mut out)?;
    Ok(out)
}

pub(crate) fn stream_body<S, T, E>(
    config: Arc<Config>,
    rows: StreamRows<S>,
) -> Result<concord_core::reqwest::Body, CodecError>
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + 'static,
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    let encoder = Encoder::new(config, rows.columns)?;
    Ok(RecordBody::new(rows.rows, encoder).into_body())
}

/// Where the `csv` writer puts its bytes; drained after every record.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Sink {
    /// Moves the written bytes to `out`; returns how many.
    fn drain_into(&self, out: &mut Vec<u8>) -> usize {
        let mut buffer = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let n = buffer.len();
        out.extend_from_slice(&buffer);
        buffer.clear();
        n
    }

    fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

impl io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

enum Schema {
    /// No row yet.
    Unknown,
    /// Rows have these field names, in order.
    Named(Vec<String>),
    /// Rows have no field names.
    Positional,
}

/// The one encoder of every write route: header policy, schema capture and checks, `csv`'s
/// serializer for the values, limits.
pub(crate) struct Encoder {
    config: Arc<Config>,
    columns: Option<Vec<String>>,
    writer: ::csv::Writer<Sink>,
    sink: Sink,
    schema: Schema,
    width: Option<usize>,
    header_written: bool,
    rows: u64,
}

impl Encoder {
    fn new(config: Arc<Config>, payload: Option<Vec<String>>) -> Result<Self, CodecError> {
        if let Some(columns) = &payload {
            check_columns(columns, config.max_fields())?;
        }
        let columns = match (payload, config.columns()) {
            (Some(payload), Some(configured)) if payload != configured => {
                return Err(CodecError::new(
                    "the payload's CSV columns conflict with the configured columns",
                ));
            }
            (Some(payload), _) => Some(payload),
            (None, configured) => configured.map(<[String]>::to_vec),
        };
        let sink = Sink::default();
        Ok(Self {
            writer: config.writer(sink.clone()),
            width: columns.as_ref().map(Vec::len),
            config,
            columns,
            sink,
            schema: Schema::Unknown,
            header_written: false,
            rows: 0,
        })
    }

    fn has_header(&self) -> bool {
        self.config.has_header()
    }

    /// Writes the header now if its names are known (explicit columns).
    fn begin(&mut self, out: &mut Vec<u8>) -> Result<(), CodecError> {
        if self.has_header()
            && let Some(columns) = self.columns.clone()
        {
            self.header(columns.iter().map(String::as_bytes), columns.len(), out)?;
        }
        Ok(())
    }

    fn header<'a>(
        &mut self,
        names: impl Iterator<Item = &'a [u8]>,
        count: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        if count > self.config.max_fields() {
            return Err(CodecError::new(
                "the CSV header has more fields than the configured limit",
            )
            .at("limit", self.config.max_fields() as u64));
        }
        let written = self
            .writer
            .write_record(names)
            .and_then(|()| self.writer.flush().map_err(Into::into));
        if let Err(e) = written {
            self.sink.clear();
            return Err(CodecError::new("the CSV header could not be written").with_source(e));
        }
        let mut header = Vec::new();
        self.sink.drain_into(&mut header);
        if header
            .len()
            .saturating_sub(self.config.write_terminator_len())
            > self.config.max_header_bytes()
        {
            return Err(
                CodecError::new("the CSV header exceeds the configured size")
                    .at("limit", self.config.max_header_bytes() as u64),
            );
        }
        out.extend_from_slice(&header);
        self.header_written = true;
        self.width = Some(count);
        Ok(())
    }

    fn raw_header(&mut self, header: &RawRecord, out: &mut Vec<u8>) -> Result<(), CodecError> {
        if let Some(columns) = &self.columns
            && !header.iter().eq(columns.iter().map(String::as_bytes))
        {
            return Err(CodecError::new(
                "the table's CSV header conflicts with the configured columns",
            ));
        }
        self.header(header.iter(), header.len(), out)?;
        self.schema = Schema::Positional;
        Ok(())
    }

    fn row<T: Serialize + ?Sized>(&mut self, row: &T, out: &mut Vec<u8>) -> Result<(), CodecError> {
        let number = self.rows + 1;
        let expect = match &self.schema {
            Schema::Named(names) => Some(names.as_slice()),
            _ => None,
        };
        let shape =
            shape(row, expect).map_err(|e| CodecError::new(e.label()).at("record", number))?;
        self.check_shape(&shape, number, out)?;
        if let Err(e) = self.writer.serialize(row) {
            self.sink.clear();
            return Err(CodecError::new("a CSV row cannot be written")
                .at("record", number)
                .with_source(e));
        }
        if let Err(e) = self.writer.flush() {
            self.sink.clear();
            return Err(CodecError::new("a CSV row cannot be written")
                .at("record", number)
                .with_source(e));
        }
        let start = out.len();
        let written = self.sink.drain_into(out);
        // The encoded record without its terminator: never smaller than its unescaped field bytes,
        // so a record a writer accepts is within a reader's limit of the same value.
        if written.saturating_sub(self.config.write_terminator_len())
            > self.config.max_record_bytes()
        {
            out.truncate(start);
            return Err(CodecError::new("a CSV record exceeds the configured size")
                .at("record", number)
                .at("limit", self.config.max_record_bytes() as u64));
        }
        self.rows = number;
        Ok(())
    }

    fn check_shape(
        &mut self,
        shape: &Shape,
        number: u64,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        if shape.fields == 0 {
            return Err(CodecError::new("a CSV row serializes to no fields").at("record", number));
        }
        if shape.fields > self.config.max_fields() {
            return Err(
                CodecError::new("a CSV row has more fields than the configured limit")
                    .at("record", number)
                    .at("limit", self.config.max_fields() as u64),
            );
        }
        match &self.schema {
            Schema::Unknown => {
                if shape.named {
                    let names: Vec<String> = shape.names.iter().map(|n| (*n).to_owned()).collect();
                    if let Some(columns) = &self.columns
                        && *columns != names
                    {
                        return Err(CodecError::new(
                            "the CSV row's field names are not the columns, in order (columns do not reorder fields)",
                        )
                        .at("record", number));
                    }
                    if check_columns(&names, usize::MAX).is_err() {
                        return Err(CodecError::new(
                            "the CSV row's field names are empty or repeated, which no header can name",
                        )
                        .at("record", number));
                    }
                    if self.has_header() && !self.header_written {
                        self.header(names.iter().map(String::as_bytes), names.len(), out)?;
                    }
                    self.schema = Schema::Named(names);
                } else {
                    if self.has_header() && !self.header_written {
                        return Err(CodecError::new(
                            "a CSV header needs column names: these rows have no field names (give explicit columns)",
                        )
                        .at("record", number));
                    }
                    self.schema = Schema::Positional;
                }
            }
            Schema::Named(_) => {
                if !shape.matches {
                    return Err(
                        CodecError::new("a CSV row changes the field names of the table")
                            .at("record", number),
                    );
                }
            }
            Schema::Positional => {
                if shape.named {
                    return Err(
                        CodecError::new("a CSV row changes the field names of the table")
                            .at("record", number),
                    );
                }
            }
        }
        if !self.config.flexible() {
            match self.width {
                Some(width) if width != shape.fields => {
                    return Err(CodecError::new(
                        "a CSV row does not have as many fields as the table",
                    )
                    .at("record", number)
                    .at("fields", shape.fields as u64)
                    .at("expected", width as u64));
                }
                Some(_) => {}
                None => self.width = Some(shape.fields),
            }
        }
        Ok(())
    }

    fn end(&mut self, _out: &mut Vec<u8>) -> Result<(), CodecError> {
        if self.has_header() && !self.header_written {
            return Err(CodecError::new(
                "an empty CSV table with a header needs explicit columns",
            ));
        }
        Ok(())
    }
}

impl<T: Serialize> RecordEncoder<T> for Encoder {
    fn begin(&mut self, out: &mut Vec<u8>) -> Result<(), CodecError> {
        Encoder::begin(self, out)
    }

    fn record(&mut self, record: &T, out: &mut Vec<u8>) -> Result<(), CodecError> {
        self.row(record, out)
    }

    fn finish(&mut self, out: &mut Vec<u8>, _records: u64) -> Result<(), CodecError> {
        self.end(out)
    }
}
