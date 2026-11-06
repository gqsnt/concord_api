use std::collections::{BTreeMap, BTreeSet};

use concord::csv::{
    Comma, Csv, CsvContext, CsvProfile, CsvProfileOptions, HasHeader, NoHeader, RawTable,
};
use concord::{CodecError, Decode, ResponseHead, bytes::Bytes};

use crate::{FORECAST_BODY_LIMIT, ForecastQuery};

/// Maximum CSV records, including both header records and the location record.
pub const CSV_RECORD_LIMIT: u64 = 10_000;

struct EnvelopeProfile;
impl CsvProfile for EnvelopeProfile {
    fn options() -> CsvProfileOptions {
        // Open-Meteo emits two tables with different widths. Parse their records with
        // Concord, then enforce each table's width and header rules below.
        CsvProfileOptions::default()
            .flexible(true)
            .max_record_bytes(256 * 1024)
            .max_total_bytes(FORECAST_BODY_LIMIT as u64)
            .max_records(CSV_RECORD_LIMIT)
    }
}

type Envelope = Csv<NoHeader, Comma, EnvelopeProfile>;

pub(crate) fn check_head(head: &ResponseHead<'_>) -> Result<(), CodecError> {
    // The document has headers; NoHeader above only disables automatic consumption of
    // the first header, so both provider tables remain available for mapping.
    <Csv<HasHeader, Comma, EnvelopeProfile> as Decode<RawTable>>::check_content_type(head)
}

/// One hourly header, including the exact spelling and its provider unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvColumn {
    /// Complete header text as decoded from CSV quoting, e.g. `temperature_2m (°C)`.
    pub header: String,
    /// Variable name before the trailing ` (unit)`; `time` has no unit.
    pub variable: String,
    /// Provider unit text, without parentheses. Unknown headers can have no unit.
    pub unit: Option<String>,
}

/// One runtime row, mapped by header name rather than column position.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvRow {
    /// ISO-8601 timestamp, with the location metadata's fixed offset.
    pub time: String,
    /// Selected numeric variables, keyed by provider name. `NaN` or empty cells are `None`.
    /// Consult [`CsvForecast::columns`] for their units.
    pub values: BTreeMap<String, Option<f64>>,
    /// Unselected columns, keyed by their complete original header, with unmodified text.
    pub unknown: BTreeMap<String, String>,
}

/// A fully validated, bounded CSV response. No rows are released before valid EOF.
#[derive(Debug, Clone)]
pub struct CsvForecast {
    bytes: Bytes,
    /// Location header names in wire order, including any extra names.
    pub location_headers: Vec<String>,
    /// Location metadata fields as text, keyed by header; no units or precision are lost.
    pub location: BTreeMap<String, String>,
    /// Hourly headers in wire order, with exact labels and parsed unit text.
    pub columns: Vec<CsvColumn>,
    /// Runtime rows; selected variables and unknown fields both remain caller-visible.
    pub rows: Vec<CsvRow>,
}
impl CsvForecast {
    /// Exact response body, including quoting, UTF-8 unit bytes and line endings.
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }
}

fn unique(names: &[String]) -> Result<(), CodecError> {
    let set: BTreeSet<_> = names.iter().collect();
    if set.len() != names.len() || names.iter().any(String::is_empty) {
        return Err(CodecError::new("CSV header has duplicate or empty names"));
    }
    Ok(())
}

pub(crate) fn decode(bytes: Bytes, query: &ForecastQuery) -> Result<CsvForecast, CodecError> {
    // The underlying CSV parser permits EOF inside quotes. Every valid CSV document
    // has an even number of quote bytes (escaped quotes are doubled). Refuse an odd
    // count before parsing so a truncated final quoted field cannot look complete.
    if !bytes
        .iter()
        .filter(|&&byte| byte == b'"')
        .count()
        .is_multiple_of(2)
    {
        return Err(CodecError::new("CSV document has an unmatched quote"));
    }
    let context = CsvContext::of::<NoHeader, Comma, EnvelopeProfile>()?;
    let table = <Envelope as Decode<RawTable>>::decode_body(context, bytes.clone())?;
    let mut records = table.records.into_iter();
    let mut next = || {
        records
            .next()
            .ok_or_else(|| {
                CodecError::new("CSV forecast is missing a location or hourly header record")
            })?
            .to_strings()
    };
    let location_headers = next()?;
    unique(&location_headers)?;
    for required in [
        "latitude",
        "longitude",
        "elevation",
        "utc_offset_seconds",
        "timezone",
        "timezone_abbreviation",
    ] {
        if !location_headers.iter().any(|name| name == required) {
            return Err(CodecError::new(
                "CSV location table is missing a required header",
            ));
        }
    }
    let location_values = next()?;
    if location_headers.len() != location_values.len() {
        return Err(CodecError::new("CSV location row has the wrong width"));
    }
    let location = location_headers
        .iter()
        .cloned()
        .zip(location_values)
        .collect();
    let headers = next()?;
    unique(&headers)?;
    let columns: Vec<_> = headers
        .into_iter()
        .map(|header| {
            let (variable, unit) = match header.strip_suffix(')').and_then(|s| s.rsplit_once(" ("))
            {
                Some((name, unit)) => (name.to_owned(), Some(unit.to_owned())),
                None => (header.clone(), None),
            };
            CsvColumn {
                header,
                variable,
                unit,
            }
        })
        .collect();
    unique(
        &columns
            .iter()
            .map(|column| column.variable.clone())
            .collect::<Vec<_>>(),
    )?;
    let time = columns
        .iter()
        .position(|column| column.header == "time")
        .ok_or_else(|| CodecError::new("CSV hourly table is missing time"))?;
    let selected: BTreeSet<_> = query.hourly.iter().map(|v| v.as_str()).collect();
    for name in &selected {
        if !columns.iter().any(|column| {
            column.variable == *name && column.unit.as_ref().is_some_and(|unit| !unit.is_empty())
        }) {
            return Err(CodecError::new(
                "CSV forecast is missing a selected variable or unit",
            ));
        }
    }
    let mut rows = Vec::new();
    for (index, record) in records.enumerate() {
        let fields = record.to_strings()?;
        if fields.len() != columns.len() {
            return Err(CodecError::new("CSV hourly row has the wrong width")
                .at("record", index as u64 + 4));
        }
        if fields[time].is_empty() {
            return Err(
                CodecError::new("CSV hourly row has an empty time").at("record", index as u64 + 4)
            );
        }
        let mut values = BTreeMap::new();
        let mut unknown = BTreeMap::new();
        for (column, field) in columns.iter().zip(&fields) {
            if selected.contains(column.variable.as_str()) {
                let value = if field == "NaN" || field.is_empty() {
                    None
                } else {
                    let value: f64 = field.parse().map_err(|error| {
                        CodecError::new("CSV selected value is not a number")
                            .at("record", index as u64 + 4)
                            .with_source(error)
                    })?;
                    if !value.is_finite() {
                        return Err(CodecError::new("CSV selected value is not finite")
                            .at("record", index as u64 + 4));
                    }
                    Some(value)
                };
                values.insert(column.variable.clone(), value);
            } else if column.header != "time" {
                unknown.insert(column.header.clone(), field.clone());
            }
        }
        rows.push(CsvRow {
            time: fields[time].clone(),
            values,
            unknown,
        });
    }
    Ok(CsvForecast {
        bytes,
        location_headers,
        location,
        columns,
        rows,
    })
}
