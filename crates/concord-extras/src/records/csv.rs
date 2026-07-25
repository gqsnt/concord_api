use std::marker::PhantomData;
use std::num::NonZeroUsize;

use concord_core::contracts::{BytesMut, ResponseHead};
use concord_core::records::{RecordDecoder, RecordError, RecordErrorKind};
use serde::de::DeserializeOwned;

const MAX_STAGED_RECORD_BYTES: usize = 1024 * 1024;

mod private {
    pub trait Sealed {}
}

/// A sealed type-level policy selecting whether a CSV stream has a header row.
///
/// Concord provides [`HasHeader`] and [`NoHeader`]. Downstream implementations
/// are intentionally unsupported; custom record formats can implement
/// [`RecordDecoder`] instead.
pub trait CsvHeaderMode: private::Sealed + Send + Sync + 'static {
    #[doc(hidden)]
    const HAS_HEADER: bool;
}

/// A sealed type-level policy selecting a built-in CSV delimiter.
///
/// Concord provides [`CommaDelimiter`], [`SemicolonDelimiter`], and
/// [`TabDelimiter`]. Downstream implementations are intentionally unsupported;
/// custom record formats can implement [`RecordDecoder`] instead.
pub trait CsvDelimiter: private::Sealed + Send + Sync + 'static {
    #[doc(hidden)]
    const BYTE: u8;
}

/// Requires and consumes one header row before decoding application records.
#[derive(Clone, Copy, Debug, Default)]
pub struct HasHeader;

/// Decodes every non-empty record positionally without consuming a header row.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHeader;

/// Selects comma-delimited CSV records.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommaDelimiter;

/// Selects semicolon-delimited CSV records.
#[derive(Clone, Copy, Debug, Default)]
pub struct SemicolonDelimiter;

/// Selects tab-delimited CSV records while retaining the `text/csv` media type.
#[derive(Clone, Copy, Debug, Default)]
pub struct TabDelimiter;

impl private::Sealed for HasHeader {}
impl private::Sealed for NoHeader {}
impl private::Sealed for CommaDelimiter {}
impl private::Sealed for SemicolonDelimiter {}
impl private::Sealed for TabDelimiter {}

impl CsvHeaderMode for HasHeader {
    const HAS_HEADER: bool = true;
}

impl CsvHeaderMode for NoHeader {
    const HAS_HEADER: bool = false;
}

impl CsvDelimiter for CommaDelimiter {
    const BYTE: u8 = b',';
}

impl CsvDelimiter for SemicolonDelimiter {
    const BYTE: u8 = b';';
}

impl CsvDelimiter for TabDelimiter {
    const BYTE: u8 = b'\t';
}

/// Typed incremental CSV decoding with type-level header and delimiter policies.
///
/// The record type `T` is the first generic parameter. Header mode `H` and
/// delimiter `D` are trailing policies so [`Csv<T>`](Csv) remains source
/// compatible and means `Csv<T, HasHeader, CommaDelimiter>`.
///
/// [`HasHeader`] consumes the first non-empty record, rejects empty or duplicate
/// header names, and uses those names when deserializing subsequent records.
/// [`NoHeader`] treats the first non-empty record as application data and
/// deserializes every record without column names. Headerless records are
/// positional, so tuples and tuple structs are recommended; named-struct
/// column matching is not available without a header.
///
/// [`CommaDelimiter`], [`SemicolonDelimiter`], and [`TabDelimiter`] select the
/// delimiter used by both incremental framing and record parsing. Every
/// configuration otherwise has the same fixed dialect and safety contract:
/// UTF-8, double-quote quoting, doubled-quote escaping, LF and CRLF record
/// endings, quoted embedded newlines, ignored empty physical lines, an optional
/// final newline, and a one-MiB staged-record limit. Parser text, headers,
/// fields, and record values are absent from errors.
///
/// Every configuration accepts and validates `text/csv`, including valid media
/// type parameters. In particular, [`TabDelimiter`] changes parsing only and
/// still validates `text/csv`, not `text/tab-separated-values`.
///
/// Configuration aliases keep endpoint declarations compact:
///
/// ```rust
/// use concord_extras::records::{
///     Csv,
///     HasHeader,
///     SemicolonDelimiter,
/// };
///
/// type ConfCsv<T> =
///     Csv<T, HasHeader, SemicolonDelimiter>;
/// ```
///
/// Headerless tab-delimited records can be named similarly:
///
/// ```rust
/// use concord_extras::records::{
///     Csv,
///     NoHeader,
///     TabDelimiter,
/// };
///
/// type HeaderlessTsv<T> =
///     Csv<T, NoHeader, TabDelimiter>;
/// ```
///
/// This type is available through the independent `csv` feature, which does
/// not enable NDJSON or the general endpoint `json` feature.
pub struct Csv<T, H = HasHeader, D = CommaDelimiter> {
    headers: Option<csv::StringRecord>,
    scan: CsvScan,
    #[allow(
        clippy::type_complexity,
        reason = "one marker must carry the record type and both policies without storing instances"
    )]
    marker: PhantomData<fn() -> (T, H, D)>,
}

#[derive(Default)]
struct CsvScan {
    index: usize,
    in_quotes: bool,
    field_start: bool,
    after_quote: bool,
}

impl CsvScan {
    const fn new() -> Self {
        Self {
            index: 0,
            in_quotes: false,
            field_start: true,
            after_quote: false,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

impl<T, H, D> Csv<T, H, D>
where
    T: DeserializeOwned + Send + 'static,
    H: CsvHeaderMode,
    D: CsvDelimiter,
{
    fn take_record(
        &mut self,
        buffer: &mut BytesMut,
        eof: bool,
    ) -> Result<Option<BytesMut>, RecordError> {
        while self.scan.index < buffer.len() {
            let index = self.scan.index;
            let byte = buffer[index];
            if self.scan.in_quotes {
                if byte != b'"' {
                    self.scan.index += 1;
                    continue;
                }
                if let Some(next) = buffer.get(index + 1) {
                    if *next == b'"' {
                        self.scan.index += 2;
                    } else {
                        self.scan.in_quotes = false;
                        self.scan.after_quote = true;
                        self.scan.index += 1;
                    }
                    continue;
                }
                if eof {
                    self.scan.in_quotes = false;
                    self.scan.after_quote = true;
                    self.scan.index += 1;
                    continue;
                }
                return Ok(None);
            }

            match byte {
                b'"' if self.scan.field_start => {
                    self.scan.in_quotes = true;
                    self.scan.field_start = false;
                    self.scan.index += 1;
                }
                b'"' => {
                    return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                }
                byte if byte == D::BYTE => {
                    self.scan.field_start = true;
                    self.scan.after_quote = false;
                    self.scan.index += 1;
                }
                b'\n' => {
                    let mut record = buffer.split_to(index + 1);
                    record.truncate(index);
                    self.scan.reset();
                    if record.is_empty() {
                        continue;
                    }
                    return Ok(Some(record));
                }
                b'\r' => {
                    let Some(next) = buffer.get(index + 1) else {
                        if eof {
                            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                        }
                        return Ok(None);
                    };
                    if *next != b'\n' {
                        return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                    }
                    let mut record = buffer.split_to(index + 2);
                    record.truncate(index);
                    self.scan.reset();
                    if record.is_empty() {
                        continue;
                    }
                    return Ok(Some(record));
                }
                _ if self.scan.after_quote => {
                    return Err(RecordError::new(RecordErrorKind::InvalidFraming));
                }
                _ => {
                    self.scan.field_start = false;
                    self.scan.index += 1;
                }
            }
        }

        if !eof {
            return Ok(None);
        }
        if self.scan.in_quotes {
            return Err(RecordError::new(RecordErrorKind::IncompleteRecord));
        }
        if buffer.is_empty() {
            return Ok(None);
        }
        let record = buffer.split_to(buffer.len());
        self.scan.reset();
        Ok(Some(record))
    }

    fn parse_record(bytes: &[u8]) -> Result<csv::StringRecord, RecordError> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(false)
            .delimiter(D::BYTE)
            .from_reader(bytes);
        let mut record = csv::StringRecord::new();
        let present = reader.read_record(&mut record).map_err(|error| {
            drop(error);
            RecordError::new(RecordErrorKind::InvalidFraming)
        })?;
        if !present {
            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
        }
        let mut extra = csv::StringRecord::new();
        if reader.read_record(&mut extra).map_err(|error| {
            drop(error);
            RecordError::new(RecordErrorKind::InvalidFraming)
        })? {
            return Err(RecordError::new(RecordErrorKind::InvalidFraming));
        }
        Ok(record)
    }

    fn consume_record(&mut self, bytes: &[u8]) -> Result<Option<T>, RecordError> {
        let record = Self::parse_record(bytes)?;
        if !H::HAS_HEADER {
            return record.deserialize(None).map(Some).map_err(|error| {
                drop(error);
                RecordError::new(RecordErrorKind::Decode)
            });
        }
        let Some(headers) = self.headers.as_ref() else {
            if record.is_empty() || record.iter().any(str::is_empty) {
                return Err(RecordError::new(RecordErrorKind::InvalidFraming));
            }
            let mut names = std::collections::HashSet::new();
            if !record.iter().all(|name| names.insert(name)) {
                return Err(RecordError::new(RecordErrorKind::InvalidFraming));
            }
            self.headers = Some(record);
            return Ok(None);
        };
        record
            .deserialize(Some(headers))
            .map(Some)
            .map_err(|error| {
                drop(error);
                RecordError::new(RecordErrorKind::Decode)
            })
    }

    fn decode_available(
        &mut self,
        buffer: &mut BytesMut,
        eof: bool,
    ) -> Result<Option<T>, RecordError> {
        while let Some(record) = self.take_record(buffer, eof)? {
            if let Some(record) = self.consume_record(&record)? {
                return Ok(Some(record));
            }
        }
        if eof && H::HAS_HEADER && self.headers.is_none() {
            return Err(RecordError::new(RecordErrorKind::IncompleteRecord));
        }
        Ok(None)
    }
}

impl<T, H, D> RecordDecoder for Csv<T, H, D>
where
    T: DeserializeOwned + Send + 'static,
    H: CsvHeaderMode,
    D: CsvDelimiter,
{
    type Record = T;

    fn accept() -> Option<&'static str> {
        Some("text/csv")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        let valid = head
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(is_csv_media_type);
        if valid {
            Ok(())
        } else {
            Err(RecordError::new(RecordErrorKind::InvalidMediaType))
        }
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self {
            headers: None,
            scan: CsvScan::new(),
            marker: PhantomData,
        })
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        self.decode_available(buffer, false)
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        self.decode_available(buffer, true)
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(MAX_STAGED_RECORD_BYTES)
            .expect("the built-in CSV staging bound is positive")
    }
}

fn is_csv_media_type(value: &str) -> bool {
    value
        .parse::<mime::Mime>()
        .is_ok_and(|media| media.type_() == mime::TEXT && media.subtype() == mime::CSV)
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use concord_core::records::RecordDecoder;
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Row {
        name: String,
        note: String,
        count: u32,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct PositionalRow(String, String, u32);

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct FramedRow(String, String, String);

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct SensitiveRow {
        #[serde(rename = "private-header-alpha")]
        alpha: String,
        #[serde(rename = "private-header-beta")]
        beta: String,
        #[serde(rename = "private-header-number")]
        number: u32,
    }

    fn decoder<T, H, D>() -> Csv<T, H, D> {
        Csv {
            headers: None,
            scan: CsvScan::new(),
            marker: PhantomData,
        }
    }

    fn feed<D>(
        mut decoder: D,
        chunks: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Vec<D::Record>, RecordError>
    where
        D: RecordDecoder,
    {
        let mut buffer = BytesMut::new();
        let mut records = Vec::new();
        for chunk in chunks {
            buffer.extend_from_slice(&chunk);
            while let Some(record) = decoder.decode(&mut buffer)? {
                records.push(record);
            }
        }
        while let Some(record) = decoder.decode_eof(&mut buffer)? {
            records.push(record);
        }
        Ok(records)
    }

    #[test]
    fn default_csv_retains_header_plus_comma_compatibility() {
        let decoder: Csv<Row> = decoder();
        let input = b"\r\nname,note,count\r\nalpha,\"comma, value\",1\nbeta,\"line one\nline \"\"two\"\"\",2\r\n\ngamma,plain,3";
        let rows = feed(decoder, input.iter().map(|byte| vec![*byte]))
            .expect("default one-byte CSV splits");
        assert_eq!(
            rows,
            [
                Row {
                    name: "alpha".into(),
                    note: "comma, value".into(),
                    count: 1,
                },
                Row {
                    name: "beta".into(),
                    note: "line one\nline \"two\"".into(),
                    count: 2,
                },
                Row {
                    name: "gamma".into(),
                    note: "plain".into(),
                    count: 3,
                },
            ]
        );
    }

    #[test]
    fn header_mode_decodes_named_structs_by_column_name_for_each_delimiter() {
        fn assert_delimiter<D>(delimiter: char)
        where
            D: CsvDelimiter,
        {
            let input =
                format!("count{delimiter}name{delimiter}note\r\n7{delimiter}alpha{delimiter}plain");
            let rows =
                feed(decoder::<Row, HasHeader, D>(), [input.into_bytes()]).expect("header CSV");
            assert_eq!(
                rows,
                [Row {
                    name: "alpha".into(),
                    note: "plain".into(),
                    count: 7,
                }]
            );
        }

        assert_delimiter::<CommaDelimiter>(',');
        assert_delimiter::<SemicolonDelimiter>(';');
        assert_delimiter::<TabDelimiter>('\t');
    }

    #[test]
    fn no_header_mode_decodes_all_three_delimiters_positionally() {
        fn assert_delimiter<D>(delimiter: char)
        where
            D: CsvDelimiter,
        {
            let input =
                format!("first{delimiter}one{delimiter}1\nsecond{delimiter}two{delimiter}2");
            let rows = feed(
                decoder::<PositionalRow, NoHeader, D>(),
                [input.into_bytes()],
            )
            .expect("headerless CSV");
            assert_eq!(
                rows,
                [
                    PositionalRow("first".into(), "one".into(), 1),
                    PositionalRow("second".into(), "two".into(), 2),
                ]
            );
        }

        assert_delimiter::<CommaDelimiter>(',');
        assert_delimiter::<SemicolonDelimiter>(';');
        assert_delimiter::<TabDelimiter>('\t');
    }

    #[test]
    fn framing_uses_each_delimiter_across_quotes_line_endings_and_chunk_boundaries() {
        fn assert_delimiter<D>(delimiter: char)
        where
            D: CsvDelimiter,
        {
            let input = format!(
                "\"left{delimiter}right\"{delimiter}\"say \"\"hello\"\"\"{delimiter}\"line one\nline two\"\r\n\
                 \"line one\r\nline two\"{delimiter}plain{delimiter}tail\r\n\
                 last{delimiter}record{delimiter}here"
            )
            .into_bytes();
            let expected = [
                FramedRow(
                    format!("left{delimiter}right"),
                    "say \"hello\"".into(),
                    "line one\nline two".into(),
                ),
                FramedRow("line one\r\nline two".into(), "plain".into(), "tail".into()),
                FramedRow("last".into(), "record".into(), "here".into()),
            ];

            let one_chunk = feed(decoder::<FramedRow, NoHeader, D>(), [input.clone()])
                .expect("several records in one chunk");
            assert_eq!(one_chunk, expected);

            let one_byte = feed(
                decoder::<FramedRow, NoHeader, D>(),
                input.iter().map(|byte| vec![*byte]),
            )
            .expect("records split at every byte");
            assert_eq!(one_byte, expected);
        }

        assert_delimiter::<CommaDelimiter>(',');
        assert_delimiter::<SemicolonDelimiter>(';');
        assert_delimiter::<TabDelimiter>('\t');
    }

    #[test]
    fn malformed_quotes_are_safely_classified_for_each_delimiter() {
        fn assert_delimiter<D>(delimiter: char)
        where
            D: CsvDelimiter,
        {
            let cases = [
                (
                    format!("left{delimiter}\"closed\"after{delimiter}right\n"),
                    RecordErrorKind::InvalidFraming,
                ),
                (
                    format!("left{delimiter}un\"quoted{delimiter}right\n"),
                    RecordErrorKind::InvalidFraming,
                ),
                (
                    format!("left{delimiter}\"unterminated-sensitive-value\n"),
                    RecordErrorKind::IncompleteRecord,
                ),
            ];
            for (input, expected) in cases {
                let error = feed(
                    decoder::<(String, String, String), NoHeader, D>(),
                    [input.into_bytes()],
                )
                .expect_err("malformed CSV must fail");
                assert_eq!(error.kind(), expected);
                let rendered = format!("{error} {error:?}");
                assert!(!rendered.contains("unterminated-sensitive-value"));
                assert!(error.source().is_none());
            }
        }

        assert_delimiter::<CommaDelimiter>(',');
        assert_delimiter::<SemicolonDelimiter>(';');
        assert_delimiter::<TabDelimiter>('\t');
    }

    #[test]
    fn header_validation_and_empty_input_follow_the_selected_mode() {
        for input in [b"name,,count\n".as_slice(), b"name,name,count\n"] {
            let error = feed(
                decoder::<Row, HasHeader, CommaDelimiter>(),
                [input.to_vec()],
            )
            .expect_err("invalid header must fail");
            assert_eq!(error.kind(), RecordErrorKind::InvalidFraming);
        }

        let empty_value = feed(
            decoder::<(String, String, String), NoHeader, CommaDelimiter>(),
            [b"name,,count\n".to_vec()],
        )
        .expect("empty data field is not header validation");
        assert_eq!(
            empty_value,
            [("name".into(), String::new(), "count".into())]
        );

        let duplicate_values = feed(
            decoder::<(String, String, String), NoHeader, CommaDelimiter>(),
            [b"name,name,count\n".to_vec()],
        )
        .expect("duplicate first-row values are data");
        assert_eq!(
            duplicate_values,
            [("name".into(), "name".into(), "count".into())]
        );

        let header_only = feed(
            decoder::<Row, HasHeader, CommaDelimiter>(),
            [b"name,note,count\n".to_vec()],
        )
        .expect("header-only input");
        assert!(header_only.is_empty());

        let missing_header = feed(decoder::<Row, HasHeader, CommaDelimiter>(), [Vec::new()])
            .expect_err("header mode requires a header");
        assert_eq!(missing_header.kind(), RecordErrorKind::IncompleteRecord);

        let empty_headerless = feed(
            decoder::<PositionalRow, NoHeader, CommaDelimiter>(),
            [Vec::new()],
        )
        .expect("empty headerless input");
        assert!(empty_headerless.is_empty());
    }

    #[test]
    fn typed_decode_failures_are_decode_errors_without_csv_values() {
        let cases = [
            feed(
                decoder::<SensitiveRow, HasHeader, CommaDelimiter>(),
                [b"private-header-alpha,private-header-beta,private-header-number\nprivate-record-alpha,private-record-beta,private-record-number\n".to_vec()],
            )
            .expect_err("typed header decode must fail"),
            feed(
                decoder::<PositionalRow, NoHeader, SemicolonDelimiter>(),
                [b"private-positional-alpha;private-positional-beta;private-positional-number\n"
                    .to_vec()],
            )
            .expect_err("typed positional decode must fail"),
        ];
        for error in cases {
            assert_eq!(error.kind(), RecordErrorKind::Decode);
            let rendered = format!("{error} {error:?}");
            for forbidden in [
                "private-header-alpha",
                "private-header-beta",
                "private-header-number",
                "private-record-alpha",
                "private-record-beta",
                "private-record-number",
                "private-positional-alpha",
                "private-positional-beta",
                "private-positional-number",
            ] {
                assert!(!rendered.contains(forbidden));
            }
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn csv_media_and_staging_contract_apply_to_every_configuration() {
        assert!(is_csv_media_type("text/csv"));
        assert!(is_csv_media_type("text/csv; charset=utf-8"));
        assert!(!is_csv_media_type("application/json"));
        assert!(!is_csv_media_type("text/tab-separated-values"));
        assert!(!is_csv_media_type("text/csv; broken=\"unterminated"));

        assert_eq!(Csv::<Row>::accept(), Some("text/csv"));
        assert_eq!(Csv::<Row>::max_buffered_bytes().get(), 1024 * 1024);
        assert_eq!(
            Csv::<Row, NoHeader, SemicolonDelimiter>::accept(),
            Some("text/csv")
        );
        assert_eq!(
            Csv::<Row, HasHeader, TabDelimiter>::max_buffered_bytes().get(),
            1024 * 1024
        );
    }
}
