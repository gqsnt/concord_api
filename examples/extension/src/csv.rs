//! The export's representation: a provider profile of Concord's reusable CSV codec
//! (`concord::csv`), chosen by this crate (the SDK's declared endpoints are JSON).
//!
//! Only the Catalog export's own rules live here; quoting, line endings, UTF-8, the BOM, limits,
//! the width rule and the head checks are the codec's.

use concord::csv::{Comma, CsvProfile, CsvProfileOptions, HasHeader};

/// The media type the export is requested in (`Accept`) and must be answered in.
pub const TEXT_CSV: &str = "text/csv";

/// One data row of a Catalog export (`id,name`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct CsvRow {
    /// Item identifier, such as `item-001`.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// The Catalog export dialect: the codec's defaults, plus the two rules of this provider:
///
/// - the header must be exactly `id,name` ([`columns`](CsvProfileOptions::columns): validated
///   against the wire header, not substituted for it);
/// - a record starting with `#` is a comment ([`comment`](CsvProfileOptions::comment)): the
///   service appends a `# ...` trailer. A `#` anywhere else is data.
#[derive(Debug, Clone, Copy)]
pub struct CatalogExport;

impl CsvProfile for CatalogExport {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .comment(Some(b'#'))
            .columns(["id", "name"])
    }
}

/// The CSV of a Catalog export, as a format: `concord::csv::Csv<HasHeader, Comma, CatalogExport>`.
/// It is `Decode<Vec<CsvRow>>` (the finite route of [`export`](crate::export)) and provides the
/// incremental reader of [`export_rows`](crate::export_rows) (`Csv::reader()`).
///
/// - **Head** (before the body is read; Concord's
///   [`ErrorKind::Metadata`](concord::ErrorKind::Metadata) with the response kept unread):
///   exactly one `Content-Type`, `text/csv`, charset absent or `utf-8` (or `us-ascii`), and a
///   `header` parameter, if any, `present`.
/// - **Body**: UTF-8 CSV records (RFC 4180 quoting, `\r\n`/`\n`/`\r`), blank lines and `#`
///   comment records skipped, one leading BOM stripped; the header exactly `id,name`; every row
///   two fields.
///
/// Every [`CodecError`](concord::CodecError) is a static label with numeric positions (`record`,
/// `field`, `fields`, `expected`): never the body's text.
pub type Csv = concord::csv::Csv<HasHeader, Comma, CatalogExport>;
