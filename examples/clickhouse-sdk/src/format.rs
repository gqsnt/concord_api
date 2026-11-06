//! The two row formats of this slice, as ClickHouse names them.

use std::fmt;

/// The response format of a query, chosen at run time.
///
/// Sent as the `X-ClickHouse-Format` request header. Since ClickHouse 26.8 the header is an
/// alias of the `output_format` setting and overrides a `FORMAT` clause in the query; before 26.8
/// it was an alias of `default_format` and a `FORMAT` clause in the query won. The SDK therefore
/// checks the `X-ClickHouse-Format` **response** header when the server sends it, and refuses a
/// response in another format ([`Error::Response`](crate::Error::Response)) before reading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    /// `CSVWithNames`: a header record of column names, then one CSV record per row (strings
    /// and dates double-quoted, a quote doubled, numbers unquoted, `\N` for NULL).
    CsvWithNames,
    /// `JSONEachRow`: one JSON object per line. 64-bit integers are JSON strings by default
    /// (`output_format_json_quote_64bit_integers`); the row type decides how to read them.
    JsonEachRow,
}

impl OutputFormat {
    /// The ClickHouse format name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::CsvWithNames => "CSVWithNames",
            Self::JsonEachRow => "JSONEachRow",
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
