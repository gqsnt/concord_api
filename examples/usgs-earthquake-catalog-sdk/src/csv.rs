use crate::{CsvRow, Error};
use concord::{CodecError, Incoming, Incremental, ResponseHead};
use concord_codecs::{
    csv::{Comma, Csv, CsvProfile, CsvProfileOptions, CsvRecords, HasHeader},
    stream::Completion,
};

/// Maximum unescaped field bytes in one CSV record (64 KiB).
pub const CSV_RECORD_LIMIT: usize = 64 * 1024;
/// Maximum decoded HTTP-body bytes in one CSV response (16 MiB).
pub const CSV_BODY_LIMIT: u64 = 16 * 1024 * 1024;

// All columns documented by USGS. Presence is required; order is free and additions are allowed.
const COLUMNS: &[&str] = &[
    "time",
    "latitude",
    "longitude",
    "depth",
    "mag",
    "magType",
    "nst",
    "gap",
    "dmin",
    "rms",
    "net",
    "id",
    "updated",
    "place",
    "type",
    "horizontalError",
    "depthError",
    "magError",
    "magNst",
    "status",
    "locationSource",
    "magSource",
];

/// Header-bearing comma CSV with explicit resource bounds.
pub struct CatalogCsvProfile;

impl CsvProfile for CatalogCsvProfile {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .max_record_bytes(CSV_RECORD_LIMIT)
            .max_header_bytes(4096)
            .max_fields(64)
            .max_total_bytes(CSV_BODY_LIMIT)
            .max_records(20_000)
    }
}

type WireCsv = Csv<HasHeader, Comma, CatalogCsvProfile>;

/// Shared CSV framing plus a provider header-presence check, including empty exports.
/// No native HTTP escape and no custom CSV parsing are involved.
pub struct CatalogCsv;

impl Incremental<CsvRow> for CatalogCsv {
    type Records = CsvRows;
    type Context = <WireCsv as Incremental<CsvRow>>::Context;
    const ACCEPT: Option<&'static str> = Some("text/csv");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <WireCsv as Incremental<CsvRow>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<Self::Context, CodecError> {
        <WireCsv as Incremental<CsvRow>>::context(head)
    }

    fn open(context: Self::Context, body: Incoming) -> Self::Records {
        CsvRows {
            inner: <WireCsv as Incremental<CsvRow>>::open(context, body),
            checked: false,
            schema_failed: false,
        }
    }
}

/// Incremental CSV rows. Each error is terminal; later reads return `None` with
/// `completion() == Failed`. Required headers are checked even with zero data rows.
#[derive(Debug)]
pub struct CsvRows {
    inner: CsvRecords<CsvRow>,
    checked: bool,
    schema_failed: bool,
}

impl CsvRows {
    /// Next row or terminal SDK error. Previously returned rows remain delivered.
    pub async fn next(&mut self) -> Option<Result<CsvRow, Error>> {
        if self.schema_failed {
            return None;
        }
        let next = self.inner.next().await;
        if let Some(Err(error)) = next {
            self.schema_failed = true;
            return Some(Err(Error::Stream(error)));
        }
        if !self.checked {
            self.checked = true;
            let valid = self.inner.headers().is_some_and(|header| {
                COLUMNS
                    .iter()
                    .all(|name| header.iter().any(|field| field == name.as_bytes()))
            });
            if !valid {
                self.schema_failed = true;
                return Some(Err(Error::Protocol(
                    "CSV header lacks required catalog columns",
                )));
            }
        }
        next.map(|row| row.map_err(Error::Stream))
    }

    /// Only `EndOfInput` establishes clean completion of this response.
    pub fn completion(&self) -> Completion {
        if self.schema_failed {
            Completion::Failed
        } else {
            self.inner.completion()
        }
    }
}
