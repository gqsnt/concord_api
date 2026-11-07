use serde::Deserialize;

/// Series metadata shared by search results and a selected series.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Series {
    /// Provider series identifier.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Inclusive real-time start date.
    pub realtime_start: String,
    /// Inclusive real-time end date.
    pub realtime_end: String,
    /// First observation date.
    pub observation_start: String,
    /// Last observation date.
    pub observation_end: String,
    /// Frequency description.
    pub frequency: String,
    /// Short frequency code.
    pub frequency_short: String,
    /// Units description.
    pub units: String,
    /// Short units description.
    pub units_short: String,
    /// Seasonal adjustment description.
    pub seasonal_adjustment: String,
    /// Short adjustment code.
    pub seasonal_adjustment_short: String,
    /// Provider timestamp, retained without timezone conversion.
    pub last_updated: String,
    /// Popularity score.
    pub popularity: u32,
    /// Group popularity, if returned.
    pub group_popularity: Option<u32>,
    /// Provider notes, if returned.
    pub notes: Option<String>,
}

/// One page of series search results.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SearchPage {
    /// Total matching records.
    pub count: u64,
    /// Offset of this response.
    pub offset: u64,
    /// Response page size.
    pub limit: u64,
    /// FRED's intentionally unusual envelope spelling.
    pub seriess: Vec<Series>,
}

/// Selected series metadata. FRED returns an array even for a single series.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Metadata {
    /// Series entries as received.
    pub seriess: Vec<Series>,
}

/// A value is always a string: `.` and other missing-value markers are preserved.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Observation {
    /// Observation date.
    pub date: String,
    /// Exact provider value; no numeric parsing or missing-value substitution.
    pub value: String,
    /// Real-time start date.
    pub realtime_start: String,
    /// Real-time end date.
    pub realtime_end: String,
}

/// An XML or JSON observations page.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Observations {
    /// Total records.
    pub count: u64,
    /// Offset of this response.
    pub offset: u64,
    /// Response page size.
    pub limit: u64,
    /// Observations in provider order.
    pub observations: Vec<Observation>,
}

/// Extracted CSV bytes and optional README. No archive path is written to disk.
/// The SDK intentionally preserves column layout and all cell strings verbatim.
#[derive(Clone, PartialEq, Eq)]
pub struct CsvExport {
    pub(crate) csv: Vec<u8>,
    pub(crate) readme: Option<Vec<u8>>,
}

impl CsvExport {
    /// CSV bytes, including headers and missing-value spelling, unchanged.
    pub fn csv(&self) -> &[u8] {
        &self.csv
    }
    /// Text README bytes, if present.
    pub fn readme(&self) -> Option<&[u8]> {
        self.readme.as_deref()
    }
    /// Consume the export for writing to an application-selected destination.
    pub fn into_csv(self) -> Vec<u8> {
        self.csv
    }
}

impl std::fmt::Debug for CsvExport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvExport")
            .field("csv_bytes", &self.csv.len())
            .field("readme_bytes", &self.readme.as_ref().map(Vec::len))
            .finish()
    }
}
