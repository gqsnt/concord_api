use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use concord::CodecError;
use serde::Deserialize;

/// Supported response representations. Selection controls both query and `Accept`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    /// `format=json`, `Accept: application/json`.
    Json,
    /// `format=csv`, `Accept: text/csv`.
    Csv,
}

impl ResponseFormat {
    pub(crate) fn accept(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Csv => "text/csv",
        }
    }
}

impl fmt::Display for ResponseFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Json => "json",
            Self::Csv => "csv",
        })
    }
}

/// An hourly variable's provider name; numeric results retain their returned units.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HourlyVariable(String);

impl HourlyVariable {
    /// Temperature at two meters (`temperature_2m`).
    pub fn temperature_2m() -> Self {
        Self("temperature_2m".into())
    }
    /// Relative humidity at two meters (`relative_humidity_2m`).
    pub fn relative_humidity_2m() -> Self {
        Self("relative_humidity_2m".into())
    }
    /// Wind speed at ten meters (`wind_speed_10m`), also used by the column perturbation.
    pub fn wind_speed_10m() -> Self {
        Self("wind_speed_10m".into())
    }
    /// Total rain, showers and snow over the preceding hour (`precipitation`).
    /// The provider's default unit is millimeters; returned unit labels are retained.
    pub fn precipitation() -> Self {
        Self("precipitation".into())
    }
    /// Select another documented hourly variable without modifying this SDK.
    /// Names must be nonempty ASCII letters, digits or underscores. Provider availability
    /// is checked by Open-Meteo; this only validates the query token's shape.
    pub fn named(name: impl Into<String>) -> Result<Self, QueryError> {
        let name = name.into();
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err(QueryError(
                "hourly variable must be a nonempty ASCII identifier",
            ));
        }
        Ok(Self(name))
    }
    /// Provider query spelling, also the JSON key and CSV column name before its unit.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One coordinate, one hourly variable list, ISO-8601 time, and provider-default units.
#[derive(Debug, Clone)]
pub struct ForecastQuery {
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    pub(crate) hourly: Vec<HourlyVariable>,
    pub(crate) timezone: Option<String>,
    pub(crate) forecast_days: Option<u8>,
}

impl ForecastQuery {
    /// Construct a single-location query. Refuses nonfinite/out-of-range coordinates,
    /// an empty selection and duplicate variables before any network activity.
    pub fn new(
        latitude: f64,
        longitude: f64,
        hourly: impl IntoIterator<Item = HourlyVariable>,
    ) -> Result<Self, QueryError> {
        if !latitude.is_finite()
            || !(-90.0..=90.0).contains(&latitude)
            || !longitude.is_finite()
            || !(-180.0..=180.0).contains(&longitude)
        {
            return Err(QueryError(
                "coordinates must be finite WGS84 latitude/longitude",
            ));
        }
        let hourly: Vec<_> = hourly.into_iter().collect();
        let unique: BTreeSet<_> = hourly.iter().collect();
        if hourly.is_empty() || unique.len() != hourly.len() {
            return Err(QueryError(
                "select at least one hourly variable, without duplicates",
            ));
        }
        Ok(Self {
            latitude,
            longitude,
            hourly,
            timezone: None,
            forecast_days: None,
        })
    }
    /// Set an IANA timezone name or `auto`; omission keeps the provider's GMT default.
    /// Open-Meteo validates this value. It is sent as one percent-encoded query value.
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = Some(timezone.into());
        self
    }
    /// Set the forecast length (0 through 16 days); omission keeps the seven-day default.
    pub fn with_forecast_days(mut self, days: u8) -> Result<Self, QueryError> {
        if days > 16 {
            return Err(QueryError("forecast_days must be between 0 and 16"));
        }
        self.forecast_days = Some(days);
        Ok(self)
    }
}

/// Invalid local query shape. No request was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryError(&'static str);
impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for QueryError {}

/// Documented single-location JSON forecast envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct Forecast {
    /// Actual grid-cell latitude, potentially different from the requested coordinate.
    pub latitude: f64,
    /// Actual grid-cell longitude.
    pub longitude: f64,
    /// Grid-cell elevation in meters; null is retained.
    pub elevation: Option<f64>,
    /// Provider generation time, in milliseconds.
    pub generationtime_ms: f64,
    /// Fixed UTC offset used for this response's timestamps.
    pub utc_offset_seconds: i32,
    /// Resolved timezone name.
    pub timezone: String,
    /// Resolved timezone abbreviation.
    pub timezone_abbreviation: String,
    /// Unit labels exactly as returned, keyed by hourly variable name.
    pub hourly_units: BTreeMap<String, String>,
    /// Timestamp and numeric arrays, including additional returned hourly variables.
    pub hourly: Hourly,
}

/// Hourly JSON arrays. Unknown hourly names stay visible in `variables`.
#[derive(Debug, Clone, Deserialize)]
pub struct Hourly {
    /// ISO-8601 timestamps, with the envelope's fixed UTC offset.
    pub time: Vec<String>,
    /// Values keyed by provider variable name. Missing observations are `None`.
    #[serde(flatten)]
    pub variables: BTreeMap<String, Vec<Option<f64>>>,
}

pub(crate) fn validate(forecast: &Forecast, query: &ForecastQuery) -> Result<(), CodecError> {
    for variable in &query.hourly {
        if !forecast.hourly.variables.contains_key(variable.as_str())
            || !forecast.hourly_units.contains_key(variable.as_str())
        {
            return Err(CodecError::new(
                "JSON forecast is missing a selected variable or unit",
            ));
        }
    }
    if forecast
        .hourly
        .variables
        .values()
        .any(|values| values.len() != forecast.hourly.time.len())
    {
        return Err(CodecError::new("JSON hourly arrays have different lengths"));
    }
    Ok(())
}
