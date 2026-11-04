//! The Salesforce Bulk API 2.0 CSV dialect on top of `concord::csv`.
//!
//! Bulk API 2.0 fixes most of its CSV dialect and lets each job choose the rest; this adapter
//! turns one job's [`BulkDialect`] into the codec's runtime [`CsvOptions`] and adds the
//! provider's header and value rules, which the generic codec does not know.
//!
//! # The dialect (from "Prepare CSV Files", Bulk API 2.0 Developer Guide)
//!
//! | Rule | Source | Here |
//! |---|---|---|
//! | "The first row in a CSV file lists the field names" | Prepare CSV Files | always [`HeaderMode::Present`]; there is no headerless Salesforce dialect (the generic codec still has `NoHeader`) |
//! | Each header name is the field's API name | Prepare CSV Files | [`FieldColumns`] refuses anything that is not an API name or a `Relationship.Field` path, repeated names and the reserved `sf__` prefix |
//! | "The header row can contain up to 32,000 characters" | Prepare CSV Files | [`FieldColumns`] refuses a longer header before encoding; the writer's header limit is set to it |
//! | Delimiters `BACKQUOTE`, `CARET`, `COMMA`, `PIPE`, `SEMICOLON`, `TAB`, chosen per job | Create a Job / Create a Query Job | the job's [`ColumnDelimiter`](crate::bulk::ColumnDelimiter); no other byte can be selected |
//! | Line ending `LF` or `CRLF`, matching the job's `lineEnding` | Line Ending Header | written with the job's ending; read accepting both (see below) |
//! | Double quotes escape delimiters, line endings and quotes | Prepare CSV Files | `"` quoting, doubled quotes, no escape byte, no comment byte |
//! | Values are not trimmed; a space next to a quote is an error | Prepare CSV Files | no trimming; the writer never pads |
//! | UTF-8 | Prepare CSV Files | typed rows are UTF-8; no BOM is written; one leading BOM is stripped on read |
//! | Empty values are ignored on update; `#N/A` sets `null` | Prepare CSV Files | [`IngestValue::Keep`] / [`IngestValue::Null`]; literal `#N/A` text is refused as ambiguous ([`FieldText`]) |
//!
//! **Reading** results: the job's delimiter; header present and, for typed rows, exactly the
//! selected fields in the query's order (Bulk API 2.0 returns columns "in the same order as you
//! requested them" from API 50.0). The codec's reader accepts `\n`, `\r\n` and `\r` terminators
//! together: it cannot be told to accept only `\r\n`, so a result body whose line ending differs
//! from the job's is parsed, not refused (`FRICTION.md` F5). Query results do not distinguish
//! `null` from an empty string (not documented either way): an empty field is `None` in typed
//! rows.

use std::error::Error as StdError;
use std::fmt;
use std::future::poll_fn;
use std::pin::Pin;

use concord::CodecError;
use concord::bytes::Bytes;
use concord::csv::{
    BomPolicy, CsvConfigError, CsvOptions, CsvReader, CsvWriter, HeaderMode, LineEnding as CsvLine,
    MediaPolicy, QuoteStyle, Rows, Trim,
};
use futures_core::Stream;
use serde::{Deserialize, Serialize, Serializer};

use crate::bulk::{BulkDialect, LineEnding};
use crate::ids::RecordId;
use crate::soql::FieldPath;

/// The documented maximum header row length, in characters.
pub const MAX_HEADER_CHARS: usize = 32_000;

/// Rows encoded per finite encoder call while spooling.
const SPOOL_BATCH: usize = 512;

/// A CSV configuration, header or encoding failure before anything was sent.
#[non_exhaustive]
pub enum CsvError {
    /// The codec refused the dialect (never expected for a [`BulkDialect`]).
    Config(CsvConfigError),
    /// The codec refused to encode (a row that does not serialize, changed fields, a limit).
    Encode(CodecError),
    /// A header name is not a Salesforce field name.
    InvalidColumn {
        /// Position in the header (0-based).
        index: usize,
        /// Why.
        reason: &'static str,
    },
    /// The header is longer than [`MAX_HEADER_CHARS`].
    HeaderTooLong {
        /// Its length.
        chars: usize,
    },
    /// An upload body over the limit, refused before sending.
    UploadTooLarge {
        /// The limit in bytes.
        limit: usize,
    },
    /// The row source failed while spooling (its error is kept, withheld from `Display`).
    Source(Box<dyn StdError + Send + Sync>),
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsvError::Config(e) => write!(f, "Salesforce CSV dialect refused: {}", e.label()),
            CsvError::Encode(e) => write!(f, "Salesforce CSV encoding failed: {e}"),
            CsvError::InvalidColumn { index, reason } => {
                write!(f, "Salesforce CSV header column {index} refused: {reason}")
            }
            CsvError::HeaderTooLong { chars } => write!(
                f,
                "Salesforce CSV header has {chars} characters (limit {MAX_HEADER_CHARS})"
            ),
            CsvError::UploadTooLarge { limit } => {
                write!(
                    f,
                    "Salesforce CSV upload larger than {limit} bytes, not sent"
                )
            }
            CsvError::Source(_) => f.write_str("the row source failed while spooling"),
        }
    }
}

impl fmt::Debug for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsvError::Config(e) => f.debug_tuple("Config").field(e).finish(),
            CsvError::Encode(e) => f.debug_tuple("Encode").field(e).finish(),
            CsvError::InvalidColumn { index, reason } => f
                .debug_struct("InvalidColumn")
                .field("index", index)
                .field("reason", reason)
                .finish(),
            CsvError::HeaderTooLong { chars } => f
                .debug_struct("HeaderTooLong")
                .field("chars", chars)
                .finish(),
            CsvError::UploadTooLarge { limit } => f
                .debug_struct("UploadTooLarge")
                .field("limit", limit)
                .finish(),
            CsvError::Source(_) => f.write_str("Source(<withheld>)"),
        }
    }
}

impl StdError for CsvError {}

impl From<CsvConfigError> for CsvError {
    fn from(e: CsvConfigError) -> Self {
        CsvError::Config(e)
    }
}

impl From<CodecError> for CsvError {
    fn from(e: CodecError) -> Self {
        CsvError::Encode(e)
    }
}

/// One job's CSV dialect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SalesforceCsv {
    dialect: BulkDialect,
}

impl SalesforceCsv {
    /// The dialect of a job created with `dialect`.
    pub const fn new(dialect: BulkDialect) -> Self {
        Self { dialect }
    }

    /// The job dialect.
    pub fn dialect(&self) -> BulkDialect {
        self.dialect
    }

    /// The codec options of this dialect (header present, the job's delimiter and line ending,
    /// the fixed quoting rules). An explicit runtime value: the codec's static `Csv<H, D, P>`
    /// types cannot carry a delimiter chosen per job.
    pub fn options(&self) -> CsvOptions {
        CsvOptions::new(HeaderMode::Present, self.dialect.delimiter.byte())
            .quote(b'"')
            .double_quote(true)
            .escape(None)
            .comment(None)
            .trim(Trim::None)
            .quote_style(QuoteStyle::Necessary)
            .bom(BomPolicy::Strip)
            .write_terminator(match self.dialect.line_ending {
                LineEnding::Lf => CsvLine::Lf,
                LineEnding::Crlf => CsvLine::CrLf,
            })
            .media(MediaPolicy::TextCsv)
    }

    /// A typed reader of result sets whose header must be exactly `columns` (the query's
    /// selected fields, in order).
    ///
    /// # Errors
    ///
    /// [`CsvError::Config`].
    pub fn result_reader(&self, columns: &[FieldPath]) -> Result<CsvReader, CsvError> {
        let names: Vec<String> = columns.iter().map(ToString::to_string).collect();
        Ok(CsvReader::new(self.options().columns(names))?)
    }

    /// A reader with the dialect and no column expectation: raw records of ingest result files
    /// (`sf__` columns followed by the uploaded fields) or of results read without a schema.
    ///
    /// # Errors
    ///
    /// [`CsvError::Config`].
    pub fn raw_reader(&self) -> Result<CsvReader, CsvError> {
        Ok(CsvReader::new(self.options())?)
    }

    /// A writer of complete Salesforce CSV bodies with this header (ingest data).
    ///
    /// # Errors
    ///
    /// [`CsvError::Config`].
    pub fn writer(&self, columns: &FieldColumns) -> Result<CsvWriter, CsvError> {
        Ok(CsvWriter::new(
            self.options()
                .columns(columns.names().iter().cloned())
                .max_header_bytes(MAX_HEADER_CHARS),
        )?)
    }

    /// The header line alone, with its terminator: the first bytes of every body of this
    /// dialect and columns.
    ///
    /// # Errors
    ///
    /// [`CsvError`].
    pub fn header(&self, columns: &FieldColumns) -> Result<Vec<u8>, CsvError> {
        let writer = self.writer(columns)?;
        Ok(writer.to_vec(Rows::new(std::iter::empty::<RawRow>()))?)
    }

    /// A writer of **data rows only** (no header), checked against `columns`, in this dialect:
    /// for bytes appended after [`header`](Self::header) in one body or in a file assembled
    /// locally (a staged result shard). Not a Salesforce dialect by itself.
    ///
    /// # Errors
    ///
    /// [`CsvError::Config`].
    pub fn rows_writer(&self, columns: &FieldColumns) -> Result<CsvWriter, CsvError> {
        let options = CsvOptions::with_profile(
            HeaderMode::Absent,
            self.dialect.delimiter.byte(),
            self.options().profile().clone(),
        )
        .columns(columns.names().iter().cloned());
        Ok(CsvWriter::new(options)?)
    }

    /// Encodes `rows` under `columns` into one body of known length, at most `limit` bytes:
    /// the header, then the rows in batches through the public finite encoder. Memory: the body
    /// so far plus one batch. Refused with [`CsvError::UploadTooLarge`] as soon as the body
    /// exceeds `limit`; nothing is sent by this call.
    ///
    /// # Errors
    ///
    /// [`CsvError`].
    pub fn spool<T, I>(
        &self,
        columns: &FieldColumns,
        rows: I,
        limit: usize,
    ) -> Result<SpooledCsv, CsvError>
    where
        T: Serialize,
        I: IntoIterator<Item = T>,
    {
        let mut spool = Spool::new(self, columns, limit)?;
        let mut batch = Vec::with_capacity(SPOOL_BATCH);
        for row in rows {
            batch.push(row);
            if batch.len() == SPOOL_BATCH {
                spool.push(&mut batch)?;
            }
        }
        spool.push(&mut batch)?;
        Ok(spool.finish())
    }

    /// [`spool`](Self::spool) from a one-shot asynchronous source, polled once, in order. A
    /// source error stops the spool ([`CsvError::Source`]); nothing has been sent.
    ///
    /// # Errors
    ///
    /// [`CsvError`].
    pub async fn spool_stream<S, T, E>(
        &self,
        columns: &FieldColumns,
        rows: S,
        limit: usize,
    ) -> Result<SpooledCsv, CsvError>
    where
        S: Stream<Item = Result<T, E>>,
        T: Serialize,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        let mut rows = Box::pin(rows);
        let mut spool = Spool::new(self, columns, limit)?;
        let mut batch = Vec::with_capacity(SPOOL_BATCH);
        while let Some(row) = poll_fn(|cx| Pin::as_mut(&mut rows).poll_next(cx)).await {
            batch.push(row.map_err(|e| CsvError::Source(e.into()))?);
            if batch.len() == SPOOL_BATCH {
                spool.push(&mut batch)?;
            }
        }
        spool.push(&mut batch)?;
        Ok(spool.finish())
    }
}

struct Spool {
    rows_writer: CsvWriter,
    body: Vec<u8>,
    rows: u64,
    limit: usize,
}

impl Spool {
    fn new(csv: &SalesforceCsv, columns: &FieldColumns, limit: usize) -> Result<Self, CsvError> {
        let body = csv.header(columns)?;
        if body.len() > limit {
            return Err(CsvError::UploadTooLarge { limit });
        }
        Ok(Self {
            rows_writer: csv.rows_writer(columns)?,
            body,
            rows: 0,
            limit,
        })
    }

    fn push<T: Serialize>(&mut self, batch: &mut Vec<T>) -> Result<(), CsvError> {
        if batch.is_empty() {
            return Ok(());
        }
        let count = batch.len() as u64;
        let bytes = self.rows_writer.to_vec(Rows::new(batch.drain(..)))?;
        if self.body.len() + bytes.len() > self.limit {
            return Err(CsvError::UploadTooLarge { limit: self.limit });
        }
        self.body.extend_from_slice(&bytes);
        self.rows += count;
        Ok(())
    }

    fn finish(self) -> SpooledCsv {
        SpooledCsv {
            body: Bytes::from(self.body),
            rows: self.rows,
        }
    }
}

/// A complete CSV body of known length, ready for one upload request.
#[derive(Clone)]
pub struct SpooledCsv {
    body: Bytes,
    rows: u64,
}

impl SpooledCsv {
    /// The body.
    pub fn body(&self) -> &Bytes {
        &self.body
    }

    /// The body, taken.
    pub fn into_body(self) -> Bytes {
        self.body
    }

    /// Data rows encoded.
    pub fn rows(&self) -> u64 {
        self.rows
    }

    /// Body bytes (what was buffered before sending).
    pub fn len(&self) -> usize {
        self.body.len()
    }

    /// Whether the body is empty (never: it has at least a header).
    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }
}

impl fmt::Debug for SpooledCsv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpooledCsv")
            .field("bytes", &self.body.len())
            .field("rows", &self.rows)
            .finish()
    }
}

/// A row without fields, only to name an element type for a header-only body.
#[derive(Serialize)]
struct RawRow;

/// A validated Salesforce CSV header (ingest data, export files): field API names or
/// `Relationship.Field` paths, distinct
/// (case-insensitively), none with the results' reserved `sf__` prefix, at most
/// [`MAX_HEADER_CHARS`] characters with delimiters.
#[derive(Clone, PartialEq, Eq)]
pub struct FieldColumns(Vec<String>);

impl FieldColumns {
    /// Validates a header.
    ///
    /// # Errors
    ///
    /// [`CsvError::InvalidColumn`], [`CsvError::HeaderTooLong`].
    pub fn new<I, S>(names: I) -> Result<Self, CsvError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names: Vec<String> = names.into_iter().map(Into::into).collect();
        if names.is_empty() {
            return Err(CsvError::InvalidColumn {
                index: 0,
                reason: "a header needs at least one field",
            });
        }
        for (index, name) in names.iter().enumerate() {
            if name.len() >= 4 && name[..4].eq_ignore_ascii_case("sf__") {
                return Err(CsvError::InvalidColumn {
                    index,
                    reason: "the sf__ prefix is reserved for result files",
                });
            }
            if FieldPath::parse(name).is_err() {
                return Err(CsvError::InvalidColumn {
                    index,
                    reason: "not a field API name or relationship path",
                });
            }
            if names[..index].iter().any(|n| n.eq_ignore_ascii_case(name)) {
                return Err(CsvError::InvalidColumn {
                    index,
                    reason: "repeated field",
                });
            }
        }
        let chars = names.iter().map(String::len).sum::<usize>() + names.len() - 1;
        if chars > MAX_HEADER_CHARS {
            return Err(CsvError::HeaderTooLong { chars });
        }
        Ok(Self(names))
    }

    /// The names, in order.
    pub fn names(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Debug for FieldColumns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FieldColumns").field(&self.0).finish()
    }
}

/// Text for an ingest field. The literal `#N/A` is refused: in Bulk API 2.0 CSV it means
/// "set to null", so as text it would be ambiguous ([`IngestValue::Null`] says it explicitly).
#[derive(Clone, PartialEq, Eq)]
pub struct FieldText(String);

impl FieldText {
    /// The text.
    ///
    /// # Errors
    ///
    /// [`crate::ids::InvalidValue`] for `#N/A`.
    pub fn new(text: impl Into<String>) -> Result<Self, crate::ids::InvalidValue> {
        let text = text.into();
        if text == NULL_MARKER {
            return Err(crate::ids::InvalidValue::new(
                "field text",
                "#N/A is Bulk API 2.0's null marker; use IngestValue::Null",
            ));
        }
        Ok(Self(text))
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for FieldText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FieldText(..)")
    }
}

impl Serialize for FieldText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

const NULL_MARKER: &str = "#N/A";

/// An ingest field value with Bulk API 2.0's empty/null semantics.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum IngestValue {
    /// An empty field: "Empty field values are ignored when you update records" (and no value
    /// on insert).
    #[default]
    Keep,
    /// `#N/A`: sets the field to `null`.
    Null,
    /// A value.
    Set(FieldText),
}

impl IngestValue {
    /// A text value.
    ///
    /// # Errors
    ///
    /// [`crate::ids::InvalidValue`] for `#N/A` (use [`IngestValue::Null`]).
    pub fn text(text: impl Into<String>) -> Result<Self, crate::ids::InvalidValue> {
        Ok(IngestValue::Set(FieldText::new(text)?))
    }
}

impl Serialize for IngestValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            IngestValue::Keep => serializer.serialize_str(""),
            IngestValue::Null => serializer.serialize_str(NULL_MARKER),
            IngestValue::Set(text) => text.serialize(serializer),
        }
    }
}

/// A Contact row of a Bulk query export: `SELECT Id, FirstName, LastName, Email, AccountId,
/// Loyalty_Tier__c FROM Contact`, in that column order ([`CONTACT_EXPORT_FIELDS`]). Empty fields
/// are `None` (the results do not distinguish `null` from empty). `Debug` shows the id only.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactExportRow {
    /// `Id`.
    #[serde(rename = "Id")]
    pub id: RecordId,
    /// `FirstName`.
    #[serde(rename = "FirstName")]
    pub first_name: Option<String>,
    /// `LastName`.
    #[serde(rename = "LastName")]
    pub last_name: String,
    /// `Email`.
    #[serde(rename = "Email")]
    pub email: Option<String>,
    /// `AccountId`.
    #[serde(rename = "AccountId")]
    pub account_id: Option<RecordId>,
    /// `Loyalty_Tier__c` (custom).
    #[serde(rename = "Loyalty_Tier__c")]
    pub loyalty_tier: Option<String>,
}

impl fmt::Debug for ContactExportRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContactExportRow")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// The columns of [`ContactExportRow`], in order.
pub const CONTACT_EXPORT_FIELDS: [&str; 6] = [
    "Id",
    "FirstName",
    "LastName",
    "Email",
    "AccountId",
    "Loyalty_Tier__c",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bulk::ColumnDelimiter;
    use concord::stream::Chunks;

    #[derive(Serialize)]
    struct Insert {
        #[serde(rename = "LastName")]
        last: FieldText,
        #[serde(rename = "Email")]
        email: IngestValue,
        #[serde(rename = "Description")]
        description: IngestValue,
    }

    fn columns() -> FieldColumns {
        FieldColumns::new(["LastName", "Email", "Description"]).unwrap()
    }

    #[test]
    fn ingest_body_uses_the_job_dialect_and_null_marker() {
        let csv = SalesforceCsv::new(BulkDialect::new(
            ColumnDelimiter::Semicolon,
            LineEnding::Crlf,
        ));
        let rows = vec![
            Insert {
                last: FieldText::new("O'Brien; Jr").unwrap(),
                email: IngestValue::Null,
                description: IngestValue::text("said \"hi\"\nthen left").unwrap(),
            },
            Insert {
                last: FieldText::new("Dury").unwrap(),
                email: IngestValue::Keep,
                description: IngestValue::Keep,
            },
        ];
        let spooled = csv.spool(&columns(), rows, 1 << 20).unwrap();
        assert_eq!(spooled.rows(), 2);
        assert_eq!(
            std::str::from_utf8(spooled.body()).unwrap(),
            "LastName;Email;Description\r\n\"O'Brien; Jr\";#N/A;\"said \"\"hi\"\"\nthen left\"\r\nDury;;\r\n"
        );
    }

    #[test]
    fn spool_is_bounded() {
        let csv = SalesforceCsv::new(BulkDialect::default());
        let rows = (0..2000).map(|i| Insert {
            last: FieldText::new(format!("name-{i}")).unwrap(),
            email: IngestValue::Keep,
            description: IngestValue::Keep,
        });
        let err = csv.spool(&columns(), rows, 4096).unwrap_err();
        assert!(matches!(err, CsvError::UploadTooLarge { limit: 4096 }));
    }

    #[test]
    fn headers_follow_salesforce_rules() {
        assert!(FieldColumns::new(["Owner.Email", "External_Id__c"]).is_ok());
        let index = |r: Result<FieldColumns, CsvError>| match r {
            Err(CsvError::InvalidColumn { index, .. }) => index,
            other => panic!("{other:?}"),
        };
        assert_eq!(index(FieldColumns::new(["Name", "sf__Id"])), 1);
        assert_eq!(index(FieldColumns::new(["Name", "name"])), 1);
        assert_eq!(index(FieldColumns::new(["Name", "Bad Name"])), 1);
        assert_eq!(index(FieldColumns::new(Vec::<String>::new())), 0);
        let long: Vec<String> = (0..4000).map(|i| format!("Field_{i:04}__c")).collect();
        assert!(matches!(
            FieldColumns::new(long),
            Err(CsvError::HeaderTooLong { .. })
        ));
        assert!(FieldText::new("#N/A").is_err());
        assert!(FieldText::new(" #N/A").is_ok());
    }

    #[tokio::test]
    async fn results_are_read_with_the_job_dialect_and_exact_header() {
        let csv = SalesforceCsv::new(BulkDialect::new(ColumnDelimiter::Pipe, LineEnding::Lf));
        let fields: Vec<FieldPath> = CONTACT_EXPORT_FIELDS
            .iter()
            .map(|f| FieldPath::parse(f).unwrap())
            .collect();
        let reader = csv.result_reader(&fields).unwrap();
        let body = "\"Id\"|\"FirstName\"|\"LastName\"|\"Email\"|\"AccountId\"|\"Loyalty_Tier__c\"\n\
                    \"003R0000000AAAAAAA\"|\"\"|\"Smith|Jones\"|\"\"|\"\"|\"Gold\"\n";
        let mut rows = reader.read::<ContactExportRow, _>(Chunks::new([body]));
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.last_name, "Smith|Jones");
        assert_eq!(row.first_name, None);
        assert_eq!(row.account_id, None);
        assert_eq!(row.loyalty_tier.as_deref(), Some("Gold"));
        assert!(rows.next().await.is_none());

        // A header that is not exactly the selected fields is a format error.
        let wrong = "Id|LastName|FirstName|Email|AccountId|Loyalty_Tier__c\n";
        let mut rows = reader.read::<ContactExportRow, _>(Chunks::new([wrong]));
        let err = rows.next().await.unwrap().unwrap_err();
        assert_eq!(err.kind(), concord::stream::ReadErrorKind::Format);
    }
}
