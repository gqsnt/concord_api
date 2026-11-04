//! Dialect, mapping, schema, limit and media options, and their validation into a [`Config`].

use std::collections::HashSet;
use std::error::Error as StdError;
use std::fmt;

use concord_core::CodecError;

/// The default [`max_record_bytes`](CsvProfileOptions::max_record_bytes): 8 MiB.
pub const DEFAULT_MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
/// The default [`max_header_bytes`](CsvProfileOptions::max_header_bytes): 1 MiB.
pub const DEFAULT_MAX_HEADER_BYTES: usize = 1024 * 1024;
/// The default [`max_fields`](CsvProfileOptions::max_fields): 4,096.
pub const DEFAULT_MAX_FIELDS: usize = 4096;

/// Whether the first parsed record is a header: the runtime counterpart of
/// [`HasHeader`](super::HasHeader) / [`NoHeader`](super::NoHeader).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeaderMode {
    /// The first parsed record is the header.
    Present,
    /// Every parsed record is data.
    Absent,
}

/// How a typed reader maps a record into a row. Never content sniffing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadMapping {
    /// From the header policy: named struct fields from the header with a header, positional
    /// without one (or the profile's [`columns`](CsvProfileOptions::columns) as names, when set
    /// for a headerless table).
    #[default]
    Default,
    /// Positional: struct fields by declaration order, tuples and sequences by position. A
    /// header is still consumed (and validated against the profile's columns, if any), but its
    /// names are not used.
    Positional,
    /// These names, for the positions of a **headerless** table. With a header it is refused as
    /// ambiguous ([`CsvConfigError`]): validate the wire header with the profile's
    /// [`columns`](CsvProfileOptions::columns) instead.
    Columns(Vec<String>),
}

impl ReadMapping {
    /// [`ReadMapping::Columns`] from any list of names.
    pub fn columns<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Columns(names.into_iter().map(Into::into).collect())
    }
}

/// The record terminator written after every record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LineEnding {
    /// `\r\n` (RFC 4180), on every platform.
    #[default]
    CrLf,
    /// `\n`.
    Lf,
}

/// The record terminators a reader accepts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RecordTerminator {
    /// `\r\n`, `\n` and `\r`.
    #[default]
    Standard,
    /// Exactly this byte (and not `\r`/`\n`, unless it is one of them).
    Byte(u8),
}

/// What a reader does with a leading UTF-8 byte order mark (`EF BB BF`). Writers never emit one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BomPolicy {
    /// Strip exactly one leading BOM; a second one is data.
    #[default]
    Strip,
    /// Refuse input that starts with a BOM (a [`Format`](crate::stream::ReadErrorKind::Format)
    /// error).
    Reject,
    /// Keep it: its bytes belong to the first field.
    Preserve,
}

/// Which parsed fields lose their leading and trailing ASCII whitespace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Trim {
    /// None: whitespace is data.
    #[default]
    None,
    /// The header's names only.
    Headers,
    /// The data fields only.
    Fields,
    /// Both.
    All,
}

/// When a writer quotes a field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuoteStyle {
    /// Only when needed: a field containing the delimiter, the quote, a terminator or (with a
    /// comment byte) starting with the comment byte, and an empty single-field record.
    #[default]
    Necessary,
    /// Every field.
    Always,
    /// Every field that is not a number.
    NonNumeric,
    /// Never, even when the output then cannot be parsed back.
    Never,
}

/// The response `Content-Type` a reader (or the finite decoder) accepts. In every accepting case
/// a present `Content-Type` that is checked must also satisfy the charset and `header=` rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MediaPolicy {
    /// Exactly one `Content-Type`, `text/csv`; charset absent or UTF-8 (or `us-ascii`); `header`
    /// absent, or once and agreeing with the header policy.
    #[default]
    TextCsv,
    /// As [`TextCsv`](Self::TextCsv), but a missing `Content-Type` is accepted.
    TextCsvOrMissing,
    /// No `Content-Type` check at all (the reader's counterpart of
    /// [`IgnoreContentType`](crate::IgnoreContentType)): for a provider that mislabels its CSV.
    /// The body is still parsed as UTF-8 CSV with every other rule.
    Unchecked,
}

/// The settings of a CSV dialect other than the header policy and the delimiter, which come from
/// the format type (`Csv<H, D, P>`) or from [`CsvOptions::new`]. Returned by
/// [`CsvProfile::options`](super::CsvProfile::options); built with the setters below from
/// [`default`](Self::default).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CsvProfileOptions {
    quote: u8,
    escape: Option<u8>,
    double_quote: bool,
    quoting: bool,
    comment: Option<u8>,
    trim: Trim,
    read_terminator: RecordTerminator,
    write_terminator: LineEnding,
    quote_style: QuoteStyle,
    bom: BomPolicy,
    read_mapping: ReadMapping,
    columns: Option<Vec<String>>,
    flexible: bool,
    max_record_bytes: usize,
    max_header_bytes: usize,
    max_fields: usize,
    max_total_bytes: Option<u64>,
    max_records: Option<u64>,
    media: MediaPolicy,
}

impl Default for CsvProfileOptions {
    fn default() -> Self {
        Self {
            quote: b'"',
            escape: None,
            double_quote: true,
            quoting: true,
            comment: None,
            trim: Trim::None,
            read_terminator: RecordTerminator::Standard,
            write_terminator: LineEnding::CrLf,
            quote_style: QuoteStyle::Necessary,
            bom: BomPolicy::Strip,
            read_mapping: ReadMapping::Default,
            columns: None,
            flexible: false,
            max_record_bytes: DEFAULT_MAX_RECORD_BYTES,
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            max_fields: DEFAULT_MAX_FIELDS,
            max_total_bytes: None,
            max_records: None,
            media: MediaPolicy::TextCsv,
        }
    }
}

/// A runtime-selected CSV dialect: a header mode and a delimiter chosen per job, plus every
/// [`CsvProfileOptions`] setting (the same setters). Validated by [`CsvReader::new`] and
/// [`CsvWriter::new`]; `Csv::<H, D, P>::options()` converts a static type into one.
///
/// [`CsvReader::new`]: super::CsvReader::new
/// [`CsvWriter::new`]: super::CsvWriter::new
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CsvOptions {
    header: HeaderMode,
    delimiter: u8,
    profile: CsvProfileOptions,
}

impl CsvOptions {
    /// The default profile with this header mode and delimiter.
    pub fn new(header: HeaderMode, delimiter: u8) -> Self {
        Self::with_profile(header, delimiter, CsvProfileOptions::default())
    }

    /// A profile's options with this header mode and delimiter.
    pub fn with_profile(header: HeaderMode, delimiter: u8, profile: CsvProfileOptions) -> Self {
        Self {
            header,
            delimiter,
            profile,
        }
    }

    /// The header mode.
    pub fn header_mode(&self) -> HeaderMode {
        self.header
    }

    /// The delimiter.
    pub fn delimiter(&self) -> u8 {
        self.delimiter
    }

    /// The other settings.
    pub fn profile(&self) -> &CsvProfileOptions {
        &self.profile
    }
}

/// Generates each setter on both option types.
macro_rules! setters {
    ($(
        $(#[$doc:meta])*
        fn $name:ident($arg:ident: $ty:ty) => $field:ident = $value:expr;
    )*) => {
        impl CsvProfileOptions {
            $(
                $(#[$doc])*
                #[must_use]
                pub fn $name(mut self, $arg: $ty) -> Self {
                    self.$field = $value;
                    self
                }
            )*
        }

        impl CsvOptions {
            $(
                $(#[$doc])*
                #[must_use]
                pub fn $name(mut self, $arg: $ty) -> Self {
                    self.profile = self.profile.$name($arg);
                    self
                }
            )*
        }
    };
}

setters! {
    /// The quote byte (default `"`).
    fn quote(quote: u8) => quote = quote;
    /// An escape byte for a quote inside a quoted field (`\"`), used instead of doubled quotes
    /// when [`double_quote`](Self::double_quote) is off; default none.
    fn escape(escape: Option<u8>) => escape = escape;
    /// Whether a quote inside a quoted field is written and read as two quotes (default on).
    /// Turning it off requires an [`escape`](Self::escape) byte.
    fn double_quote(yes: bool) => double_quote = yes;
    /// Whether the reader treats the quote byte specially at all (default on). Writers always
    /// quote per [`quote_style`](Self::quote_style).
    fn quoting(yes: bool) => quoting = yes;
    /// A comment byte: a record starting with it is skipped by readers (writers quote a field
    /// that starts with it). Default none: `#` is ordinary data.
    fn comment(comment: Option<u8>) => comment = comment;
    /// Whitespace trimming of parsed fields (default none).
    fn trim(trim: Trim) => trim = trim;
    /// The record terminators readers accept (default `\r\n`, `\n`, `\r`).
    fn read_terminator(terminator: RecordTerminator) => read_terminator = terminator;
    /// The record terminator writers emit (default `\r\n`).
    fn write_terminator(ending: LineEnding) => write_terminator = ending;
    /// When writers quote (default only when necessary).
    fn quote_style(style: QuoteStyle) => quote_style = style;
    /// The leading byte order mark policy of readers (default strip one).
    fn bom(policy: BomPolicy) => bom = policy;
    /// How typed readers map records into rows (default from the header policy).
    fn read_mapping(mapping: ReadMapping) => read_mapping = mapping;
    /// Whether records may differ in width (default no: every record has the width of the
    /// header, the supplied columns or the first record).
    fn flexible(yes: bool) => flexible = yes;
    /// The most unescaped field bytes of one data record (default
    /// [`DEFAULT_MAX_RECORD_BYTES`]); at least 1. Readers never buffer more. Writers refuse a
    /// record whose encoded form (quotes and delimiters included, terminator excluded) is larger,
    /// so what a writer accepts is within a reader's limit of the same value.
    fn max_record_bytes(bytes: usize) => max_record_bytes = bytes;
    /// The most unescaped field bytes of the header record (default
    /// [`DEFAULT_MAX_HEADER_BYTES`]); at least 1. Writers apply it like
    /// [`max_record_bytes`](Self::max_record_bytes).
    fn max_header_bytes(bytes: usize) => max_header_bytes = bytes;
    /// The most fields of one record, header included (default [`DEFAULT_MAX_FIELDS`]); at
    /// least 1.
    fn max_fields(fields: usize) => max_fields = fields;
    /// The most input bytes a reader consumes (default no limit); reaching it is a
    /// [`Limit`](crate::stream::ReadErrorKind::Limit) error, not an end.
    fn max_total_bytes(bytes: u64) => max_total_bytes = Some(bytes);
    /// The most data records a reader delivers (default no limit); one more is a
    /// [`Limit`](crate::stream::ReadErrorKind::Limit) error, not an end.
    fn max_records(records: u64) => max_records = Some(records);
    /// The response `Content-Type` policy of readers and the finite decoder (default exactly one
    /// `text/csv`).
    fn media(policy: MediaPolicy) => media = policy;
}

impl CsvProfileOptions {
    /// The table's columns, in order:
    ///
    /// - writers with a header emit them as the header (named rows must emit exactly these
    ///   fields, in this order; positional rows are labelled by them); a payload's own different
    ///   columns are refused, not preferred;
    /// - writers without a header check named rows against them;
    /// - readers with a header require the wire header to be exactly these names (a
    ///   [`Format`](crate::stream::ReadErrorKind::Format) error while reading, not a substitute
    ///   for the wire names);
    /// - readers without a header use them as the names of the positions with
    ///   [`ReadMapping::Default`], and as the table width.
    ///
    /// Names must be non-empty and distinct.
    #[must_use]
    pub fn columns<I, S>(mut self, columns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.columns = Some(columns.into_iter().map(Into::into).collect());
        self
    }
}

impl CsvProfileOptions {
    pub(crate) fn media_policy(&self) -> MediaPolicy {
        self.media
    }
}

impl CsvOptions {
    /// The table's columns: see [`CsvProfileOptions::columns`].
    #[must_use]
    pub fn columns<I, S>(mut self, columns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.profile = self.profile.columns(columns);
        self
    }
}

/// An invalid CSV configuration, found before anything is read or sent: a delimiter that is a
/// record terminator or equals the quote, colliding quote/escape/comment bytes, a byte above
/// `0x7f`, a zero limit, an ambiguous or repeated column list.
///
/// A static description only. On the [`Encode`](concord_core::Encode) and
/// [`Decode`](concord_core::Decode) routes it becomes a [`CodecError`] with the same label
/// (a `Build` error before sending, a `Metadata` error before the body is read).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CsvConfigError {
    label: &'static str,
}

impl CsvConfigError {
    pub(crate) const fn new(label: &'static str) -> Self {
        Self { label }
    }

    /// The static description.
    pub fn label(&self) -> &'static str {
        self.label
    }
}

impl fmt::Display for CsvConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid CSV configuration: {}", self.label)
    }
}

impl fmt::Debug for CsvConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CsvConfigError")
            .field("label", &self.label)
            .finish()
    }
}

impl StdError for CsvConfigError {}

impl From<CsvConfigError> for CodecError {
    fn from(error: CsvConfigError) -> Self {
        CodecError::new(error.label)
    }
}

/// Validated options, resolved once per reader, writer or call.
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) options: CsvOptions,
    /// The names of a headerless table's positions (mapping `Columns`, or the profile's columns
    /// with mapping `Default`).
    pub(crate) supplied: Option<::csv::StringRecord>,
    /// The width a headerless table must have (supplied or profile columns).
    pub(crate) width: Option<usize>,
}

impl Config {
    pub(crate) fn new(options: CsvOptions) -> Result<Self, CsvConfigError> {
        validate(&options)?;
        let p = &options.profile;
        let has_header = options.header == HeaderMode::Present;
        let mut supplied = None;
        let mut width = None;
        if !has_header {
            let names = match &p.read_mapping {
                ReadMapping::Columns(names) => Some(names),
                ReadMapping::Default => p.columns.as_ref(),
                ReadMapping::Positional => None,
            };
            supplied = names.map(|names| ::csv::StringRecord::from(names.clone()));
            width = names.or(p.columns.as_ref()).map(Vec::len);
        }
        Ok(Self {
            options,
            supplied,
            width,
        })
    }

    pub(crate) fn has_header(&self) -> bool {
        self.options.header == HeaderMode::Present
    }

    pub(crate) fn columns(&self) -> Option<&[String]> {
        self.options.profile.columns.as_deref()
    }

    pub(crate) fn positional(&self) -> bool {
        matches!(self.options.profile.read_mapping, ReadMapping::Positional)
    }

    pub(crate) fn content_type(&self) -> &'static str {
        super::content_type(self.has_header())
    }

    /// The framing parser of this dialect.
    pub(crate) fn core_reader(&self) -> csv_core::Reader {
        let p = &self.options.profile;
        csv_core::ReaderBuilder::new()
            .delimiter(self.options.delimiter)
            .quote(p.quote)
            .escape(p.escape)
            .double_quote(p.double_quote)
            .quoting(p.quoting)
            .comment(p.comment)
            .terminator(match p.read_terminator {
                RecordTerminator::Standard => csv_core::Terminator::CRLF,
                RecordTerminator::Byte(b) => csv_core::Terminator::Any(b),
            })
            .build()
    }

    /// The writer of this dialect (headers are written by the caller).
    pub(crate) fn writer<W: std::io::Write>(&self, sink: W) -> ::csv::Writer<W> {
        let p = &self.options.profile;
        let mut builder = ::csv::WriterBuilder::new();
        builder
            .has_headers(false)
            .delimiter(self.options.delimiter)
            .quote(p.quote)
            .double_quote(p.double_quote)
            .comment(p.comment)
            .flexible(p.flexible)
            .terminator(match p.write_terminator {
                LineEnding::CrLf => ::csv::Terminator::CRLF,
                LineEnding::Lf => ::csv::Terminator::Any(b'\n'),
            })
            .quote_style(match p.quote_style {
                QuoteStyle::Necessary => ::csv::QuoteStyle::Necessary,
                QuoteStyle::Always => ::csv::QuoteStyle::Always,
                QuoteStyle::NonNumeric => ::csv::QuoteStyle::NonNumeric,
                QuoteStyle::Never => ::csv::QuoteStyle::Never,
            });
        if let Some(escape) = p.escape {
            builder.escape(escape);
        }
        builder.from_writer(sink)
    }

    pub(crate) fn comment(&self) -> Option<u8> {
        self.options.profile.comment
    }

    /// Whether the reader ends a record (or a comment line) at `b`.
    pub(crate) fn is_terminator(&self, b: u8) -> bool {
        match self.options.profile.read_terminator {
            RecordTerminator::Standard => b == b'\r' || b == b'\n',
            RecordTerminator::Byte(t) => b == t,
        }
    }

    pub(crate) fn write_terminator_len(&self) -> usize {
        match self.options.profile.write_terminator {
            LineEnding::CrLf => 2,
            LineEnding::Lf => 1,
        }
    }

    pub(crate) fn trim_headers(&self) -> bool {
        matches!(self.options.profile.trim, Trim::Headers | Trim::All)
    }

    pub(crate) fn trim_fields(&self) -> bool {
        matches!(self.options.profile.trim, Trim::Fields | Trim::All)
    }

    pub(crate) fn bom(&self) -> BomPolicy {
        self.options.profile.bom
    }

    pub(crate) fn flexible(&self) -> bool {
        self.options.profile.flexible
    }

    pub(crate) fn max_record_bytes(&self) -> usize {
        self.options.profile.max_record_bytes
    }

    pub(crate) fn max_header_bytes(&self) -> usize {
        self.options.profile.max_header_bytes
    }

    pub(crate) fn max_fields(&self) -> usize {
        self.options.profile.max_fields
    }

    pub(crate) fn max_total_bytes(&self) -> Option<u64> {
        self.options.profile.max_total_bytes
    }

    pub(crate) fn max_records(&self) -> Option<u64> {
        self.options.profile.max_records
    }

    pub(crate) fn media(&self) -> MediaPolicy {
        self.options.profile.media
    }
}

/// Checks a column list: non-empty, every name non-empty and distinct.
pub(crate) fn check_columns(columns: &[String], max_fields: usize) -> Result<(), CsvConfigError> {
    if columns.is_empty() {
        return Err(CsvConfigError::new("a CSV column list is empty"));
    }
    if columns.len() > max_fields {
        return Err(CsvConfigError::new(
            "a CSV column list has more names than the field limit",
        ));
    }
    let mut seen = HashSet::with_capacity(columns.len());
    for name in columns {
        if name.is_empty() {
            return Err(CsvConfigError::new("a CSV column name is empty"));
        }
        if !seen.insert(name.as_str()) {
            return Err(CsvConfigError::new("a CSV column list repeats a name"));
        }
    }
    Ok(())
}

fn validate(options: &CsvOptions) -> Result<(), CsvConfigError> {
    let p = &options.profile;
    let d = options.delimiter;
    let terminator = |b: u8| b == b'\r' || b == b'\n';
    let ascii = d.is_ascii()
        && p.quote.is_ascii()
        && p.escape.is_none_or(|e| e.is_ascii())
        && p.comment.is_none_or(|c| c.is_ascii())
        && match p.read_terminator {
            RecordTerminator::Standard => true,
            RecordTerminator::Byte(t) => t.is_ascii(),
        };
    if !ascii {
        return Err(CsvConfigError::new(
            "a CSV delimiter, quote, escape, comment or terminator must be one ASCII byte (multi-byte separators are not supported)",
        ));
    }
    if terminator(d) {
        return Err(CsvConfigError::new(
            "the CSV delimiter is a record terminator (\\r or \\n)",
        ));
    }
    if terminator(p.quote) {
        return Err(CsvConfigError::new(
            "the CSV quote is a record terminator (\\r or \\n)",
        ));
    }
    if d == p.quote {
        return Err(CsvConfigError::new("the CSV delimiter equals the quote"));
    }
    if let Some(e) = p.escape {
        if terminator(e) {
            return Err(CsvConfigError::new(
                "the CSV escape is a record terminator (\\r or \\n)",
            ));
        }
        if e == d {
            return Err(CsvConfigError::new("the CSV escape equals the delimiter"));
        }
        if e == p.quote {
            return Err(CsvConfigError::new(
                "the CSV escape equals the quote (use double_quote)",
            ));
        }
    }
    if !p.double_quote && p.escape.is_none() {
        return Err(CsvConfigError::new(
            "CSV double_quote(false) needs an escape byte",
        ));
    }
    if let Some(c) = p.comment {
        if terminator(c) {
            return Err(CsvConfigError::new(
                "the CSV comment byte is a record terminator (\\r or \\n)",
            ));
        }
        if c == d || c == p.quote || Some(c) == p.escape {
            return Err(CsvConfigError::new(
                "the CSV comment byte equals the delimiter, the quote or the escape",
            ));
        }
    }
    if let RecordTerminator::Byte(t) = p.read_terminator
        && (t == d || t == p.quote || Some(t) == p.escape || Some(t) == p.comment)
    {
        return Err(CsvConfigError::new(
            "the CSV record terminator equals the delimiter, the quote, the escape or the comment byte",
        ));
    }
    if p.max_record_bytes == 0 || p.max_header_bytes == 0 || p.max_fields == 0 {
        return Err(CsvConfigError::new(
            "a CSV record, header or field limit is zero",
        ));
    }
    if let Some(columns) = &p.columns {
        check_columns(columns, p.max_fields)?;
    }
    if let ReadMapping::Columns(names) = &p.read_mapping {
        if options.header == HeaderMode::Present {
            return Err(CsvConfigError::new(
                "ReadMapping::Columns with a header is ambiguous: the header names the columns (validate it with the profile's columns)",
            ));
        }
        check_columns(names, p.max_fields)?;
        if p.columns.as_ref().is_some_and(|columns| columns != names) {
            return Err(CsvConfigError::new(
                "ReadMapping::Columns conflicts with the profile's columns",
            ));
        }
    }
    Ok(())
}
