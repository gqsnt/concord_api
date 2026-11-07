//! Writing: [`Rows`], [`StreamRows`], [`CsvWriter`], and the one encoder behind the finite and
//! lazy routes.

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use concord_core::CodecError;
use concord_core::reqwest::header::HeaderValue;
use futures_core::Stream;
use serde::Serialize;

use super::fields::{Dialect, Exceeded, Fields, FloatText};
use super::names::{Shape, ShapeError, encode};
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
/// use concord_codecs::csv::{Csv, Rows};
///
/// let writer = Csv::<concord_codecs::csv::HasHeader>::writer().unwrap();
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
/// static [`Encode::CONTENT_TYPE`](concord_core::Encode::CONTENT_TYPE) cannot express:
/// [`upload`](Self::upload) returns the body together with its `Content-Type` ([`Upload`]). Send
/// it as a declared body with the [`Encoded`](crate::Encoded) format (`body rows: Upload` with
/// `request Encoded;`), which keeps the two together; [`Upload::apply`] is for a native request.
/// A table whose header is written from configuration needs its columns
/// ([`StreamRows::with_columns`]) when the source may be empty.
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

enum Schema {
    /// No row yet.
    Unknown,
    /// Rows have these field names, in order: the serializer's own names, never copied.
    Named(Vec<&'static str>),
    /// Rows have no field names.
    Positional,
}

/// The one encoder of every write route: header policy, schema capture and checks, the record
/// encoding, limits.
///
/// A row is serialized once, into `record`, which never holds more than the record limit plus the
/// terminator; it reaches `out` only once every check passed, so a refused row leaves `out` as it
/// was. Header names (configured columns, inferred field names) are borrowed, never copied: the
/// header's only buffer is its bounded encoding.
pub(crate) struct Encoder {
    config: Arc<Config>,
    dialect: Dialect,
    floats: FloatText,
    /// The payload's explicit columns, when there are no configured ones (an equal payload list is
    /// dropped for the configured one, which is read from `config`).
    columns: Option<Vec<String>>,
    record: Vec<u8>,
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
            // Configured columns (an equal payload list is the same table) are read from `config`.
            (_, Some(_)) => None,
            (payload, None) => payload,
        };
        Ok(Self {
            dialect: Dialect::new(&config),
            floats: FloatText::new(),
            width: columns.as_deref().or(config.columns()).map(<[String]>::len),
            config,
            columns,
            record: Vec::new(),
            schema: Schema::Unknown,
            header_written: false,
            rows: 0,
        })
    }

    fn has_header(&self) -> bool {
        self.config.has_header()
    }

    /// The table's explicit columns: the payload's or the configured ones.
    fn column_names(&self) -> Option<&[String]> {
        self.columns.as_deref().or(self.config.columns())
    }

    /// Writes the header now if its names are known (explicit columns), from the borrowed list.
    fn begin(&mut self, out: &mut Vec<u8>) -> Result<(), CodecError> {
        if !self.has_header() {
            return Ok(());
        }
        let config = Arc::clone(&self.config);
        let payload = self.columns.take();
        let written = match payload.as_deref().or(config.columns()) {
            Some(columns) => self.header(columns.iter().map(String::as_bytes), columns.len(), out),
            None => Ok(()),
        };
        self.columns = payload;
        written
    }

    /// Encodes the header into its own buffer (never more than the header limit plus the
    /// terminator), then appends it to `out`.
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
        let limit = self.config.max_header_bytes();
        let too_large = |Exceeded| {
            CodecError::new("the CSV header exceeds the configured size").at("limit", limit as u64)
        };
        let mut header = Vec::new();
        let mut fields = Fields::new(&self.dialect, &mut self.floats, &mut header, limit);
        for name in names {
            fields.field(name).map_err(too_large)?;
        }
        fields.end().map_err(too_large)?;
        out.extend_from_slice(&header);
        self.header_written = true;
        self.width = Some(count);
        Ok(())
    }

    fn raw_header(&mut self, header: &RawRecord, out: &mut Vec<u8>) -> Result<(), CodecError> {
        if let Some(columns) = self.column_names()
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
        // The encoded record without its terminator: never smaller than its unescaped field bytes,
        // so a record a writer accepts is within a reader's limit of the same value.
        let mut fields = Fields::new(
            &self.dialect,
            &mut self.floats,
            &mut self.record,
            self.config.max_record_bytes(),
        );
        let shape = encode(row, expect, self.config.max_fields(), &mut fields)
            .and_then(|shape| {
                fields.end()?;
                Ok(shape)
            })
            .map_err(|e| row_error(&self.config, e, number))?;
        self.accept(shape, number, out)?;
        out.extend_from_slice(&self.record);
        self.rows = number;
        Ok(())
    }

    /// The table checks of an encoded row; writes an inferred header. Nothing is written when a
    /// check fails.
    fn accept(&mut self, shape: Shape, number: u64, out: &mut Vec<u8>) -> Result<(), CodecError> {
        if shape.fields == 0 {
            return Err(CodecError::new("a CSV row serializes to no fields").at("record", number));
        }
        let mut header = false;
        let schema = match &self.schema {
            Schema::Unknown => {
                if shape.named {
                    let names = shape.names;
                    if let Some(columns) = self.column_names()
                        && !columns.iter().map(String::as_str).eq(names.iter().copied())
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
                    header = self.has_header() && !self.header_written;
                    Some(Schema::Named(names))
                } else {
                    if self.has_header() && !self.header_written {
                        return Err(CodecError::new(
                            "a CSV header needs column names: these rows have no field names (give explicit columns)",
                        )
                        .at("record", number));
                    }
                    Some(Schema::Positional)
                }
            }
            Schema::Named(_) => {
                if !shape.matches {
                    return Err(
                        CodecError::new("a CSV row changes the field names of the table")
                            .at("record", number),
                    );
                }
                None
            }
            Schema::Positional => {
                if shape.named {
                    return Err(
                        CodecError::new("a CSV row changes the field names of the table")
                            .at("record", number),
                    );
                }
                None
            }
        };
        // An inferred header has the row's width (its names are the row's fields).
        let width = self.width.or(header.then_some(shape.fields));
        if !self.config.flexible()
            && let Some(width) = width
            && width != shape.fields
        {
            return Err(
                CodecError::new("a CSV row does not have as many fields as the table")
                    .at("record", number)
                    .at("fields", shape.fields as u64)
                    .at("expected", width as u64),
            );
        }
        // The inferred header, encoded (bounded) from the serializer's names.
        if header && let Some(Schema::Named(names)) = &schema {
            self.header(names.iter().map(|n| n.as_bytes()), names.len(), out)?;
        }
        if let Some(schema) = schema {
            self.schema = schema;
        }
        if !self.config.flexible() && self.width.is_none() {
            self.width = Some(shape.fields);
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

/// The `CodecError` of a row that could not be encoded.
fn row_error(config: &Config, error: ShapeError, number: u64) -> CodecError {
    let codec = CodecError::new(error.label()).at("record", number);
    match error {
        ShapeError::Fields => codec.at("limit", config.max_fields() as u64),
        ShapeError::Bytes => codec.at("limit", config.max_record_bytes() as u64),
        ShapeError::Custom(_) => codec.with_source(error),
        ShapeError::Shape(_) => codec,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv::HeaderMode;

    fn encoder(options: CsvOptions, columns: Option<Vec<String>>) -> Encoder {
        Encoder::new(Arc::new(Config::new(options).unwrap()), columns).unwrap()
    }

    fn absent() -> CsvOptions {
        CsvOptions::new(HeaderMode::Absent, b',')
    }

    #[test]
    fn a_refused_record_leaves_the_frame_untouched_and_the_buffer_within_the_limit() {
        let mut e = encoder(absent().max_record_bytes(64), None);
        let mut out = Vec::new();
        e.row(&("a", "b"), &mut out).unwrap();
        let (before, capacity) = (out.clone(), out.capacity());

        // One field over the limit, and a record that crosses it at its second field.
        let big = "x".repeat(1 << 20);
        for row in [(big.as_str(), "b"), ("a", big.as_str())] {
            let error = e.row(&row, &mut out).unwrap_err();
            assert_eq!(error.label(), "a CSV record exceeds the configured size");
            assert_eq!(out, before);
            assert_eq!(
                out.capacity(),
                capacity,
                "nothing was copied into the frame"
            );
            assert!(e.record.capacity() <= 64 + 2, "{}", e.record.capacity());
        }
        let half = "y".repeat(40);
        let error = e
            .row(&(half.as_str(), half.as_str()), &mut out)
            .unwrap_err();
        assert_eq!(error.get("limit"), Some(64));
        assert_eq!(out, before);
        assert!(e.record.capacity() <= 64 + 2, "{}", e.record.capacity());
    }

    #[test]
    fn the_record_buffer_grows_to_the_limit_and_no_further() {
        let limit = 1000;
        let mut e = encoder(absent().max_record_bytes(limit).flexible(true), None);
        let mut out = Vec::new();
        // Records of every size up to the limit (quoted, escaped, many fields), then over it.
        for n in 0..=limit {
            let field = "\"".repeat(n / 2);
            let encoded = n / 2 * 2 + 2;
            let result = e.row(&(field.as_str(),), &mut out);
            assert_eq!(result.is_ok(), encoded <= limit, "{n}");
            assert!(e.record.capacity() <= limit + 2, "{}", e.record.capacity());
            let fields: Vec<&str> = vec!["z"; n.max(1)];
            let _ = e.row(&fields, &mut out);
            assert!(e.record.capacity() <= limit + 2, "{}", e.record.capacity());
        }
    }

    #[test]
    fn a_refused_header_never_reaches_the_frame() {
        let name = "h".repeat(1 << 20);
        let mut e = encoder(
            CsvOptions::new(HeaderMode::Present, b',').max_header_bytes(64),
            Some(vec![name]),
        );
        let mut out = Vec::new();
        let error = e.begin(&mut out).unwrap_err();
        assert_eq!(error.label(), "the CSV header exceeds the configured size");
        assert_eq!(error.get("limit"), Some(64));
        assert_eq!((out.len(), out.capacity()), (0, 0));
        assert!(!e.header_written);
    }

    // Header names are borrowed, never copied (ASTRA-2 re-check): the header's only buffer is
    // its bounded encoding.

    /// A struct row with one field, named at run time.
    struct Named(&'static str);

    impl Serialize for Named {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeStruct;
            let mut row = serializer.serialize_struct("Named", 1)?;
            row.serialize_field(self.0, &1)?;
            row.end()
        }
    }

    fn long_name() -> &'static str {
        Box::leak("n".repeat(1 << 16).into_boxed_str())
    }

    fn present() -> CsvOptions {
        CsvOptions::new(HeaderMode::Present, b',')
    }

    #[test]
    fn inferred_names_are_the_serializers_names_not_copies() {
        let name = long_name();
        for options in [present().max_header_bytes(1 << 20), absent()] {
            let mut e = encoder(options, None);
            let mut out = Vec::new();
            e.row(&Named(name), &mut out).unwrap();
            e.row(&Named(name), &mut out).unwrap();
            match &e.schema {
                Schema::Named(names) => {
                    assert_eq!(names.len(), 1);
                    assert!(
                        std::ptr::eq(names[0].as_ptr(), name.as_ptr()),
                        "the schema keeps the serializer's name, not a copy"
                    );
                }
                _ => panic!("a named schema"),
            }
        }

        // Over the header limit: refused, nothing written, nothing kept.
        let mut e = encoder(present().max_header_bytes(64), None);
        let mut out = Vec::new();
        let error = e.row(&Named(name), &mut out).unwrap_err();
        assert_eq!(error.label(), "the CSV header exceeds the configured size");
        assert_eq!(error.get("limit"), Some(64));
        assert_eq!((out.len(), out.capacity()), (0, 0));
        assert!(matches!(e.schema, Schema::Unknown));
        assert!(!e.header_written);
    }

    #[test]
    fn configured_columns_are_read_from_the_configuration_not_copied() {
        let name = long_name().to_owned();
        let config = Arc::new(Config::new(present().columns([name.clone()])).unwrap());
        for payload in [None, Some(vec![name.clone()])] {
            let mut e = Encoder::new(Arc::clone(&config), payload).unwrap();
            assert!(
                e.columns.is_none(),
                "the configured list is borrowed from the configuration"
            );
            let mut out = Vec::new();
            e.begin(&mut out).unwrap();
            assert_eq!(out.len(), name.len() + 2);
            e.row(&(1,), &mut out).unwrap();
        }

        // Over the header limit: refused before anything reaches the frame.
        let config =
            Arc::new(Config::new(present().columns([name.clone()]).max_header_bytes(64)).unwrap());
        let mut e = Encoder::new(config, None).unwrap();
        assert!(e.columns.is_none());
        let mut out = Vec::new();
        let error = e.begin(&mut out).unwrap_err();
        assert_eq!(error.get("limit"), Some(64));
        assert_eq!((out.len(), out.capacity()), (0, 0));

        // Payload columns without configured ones are the payload's own list (moved, not copied).
        let mut e = encoder(present(), Some(vec![name.clone()]));
        let kept = e.columns.as_ref().unwrap()[0].as_ptr();
        let mut out = Vec::new();
        e.begin(&mut out).unwrap();
        assert!(std::ptr::eq(e.columns.as_ref().unwrap()[0].as_ptr(), kept));
    }
}
