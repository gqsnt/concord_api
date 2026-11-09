use crate::Error;
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde::Deserialize;
use std::fmt;

/// An RFC 3339 timestamp with an explicit offset, normalized to UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// Parse an RFC 3339 instant. Date-only and timezone-free inputs are outside this slice.
    pub fn parse(value: &str) -> Result<Self, Error> {
        DateTime::parse_from_rfc3339(value)
            .map(|value| Self(value.with_timezone(&Utc)))
            .map_err(|_| Error::InvalidInput("timestamp must be RFC 3339 with an offset"))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_rfc3339_opts(SecondsFormat::AutoSi, true))
    }
}

/// A finite magnitude bound; this type does not perform scientific validation.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Magnitude(f64);

impl Magnitude {
    /// Reject NaN and infinities before they can reach the wire.
    pub fn new(value: f64) -> Result<Self, Error> {
        if !value.is_finite() {
            return Err(Error::InvalidInput("magnitude must be finite"));
        }
        Ok(Self(value))
    }
}

impl fmt::Display for Magnitude {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub(crate) fn validate_magnitude_bounds(
    min: Option<Magnitude>,
    max: Option<Magnitude>,
) -> Result<(), Error> {
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return Err(Error::InvalidInput(
            "minimum magnitude must not exceed maximum magnitude",
        ));
    }
    Ok(())
}

/// The four `orderby` values supported by the USGS FDSN event query.
/// See <https://earthquake.usgs.gov/fdsnws/event/1/#parameters> (read 2026-11-07).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OrderBy {
    /// Origin time, newest first (`time`).
    Time,
    /// Origin time, oldest first (`time-asc`); the SDK's compatibility default.
    #[default]
    TimeAsc,
    /// Largest magnitude first (`magnitude`).
    Magnitude,
    /// Smallest magnitude first (`magnitude-asc`).
    MagnitudeAsc,
}

impl fmt::Display for OrderBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Time => "time",
            Self::TimeAsc => "time-asc",
            Self::Magnitude => "magnitude",
            Self::MagnitudeAsc => "magnitude-asc",
        })
    }
}

/// USGS's inclusive per-response result limit, 1 through 20,000.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(u16);

impl PageSize {
    /// The SDK default: 500 events.
    pub const DEFAULT: Self = Self(500);

    /// Validate the provider's limit range.
    pub fn new(value: u16) -> Result<Self, Error> {
        if !(1..=20_000).contains(&value) {
            return Err(Error::InvalidInput("page size must be between 1 and 20000"));
        }
        Ok(Self(value))
    }

    /// The wire value.
    pub fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A closed time interval of at most 31 days, plus filters shared by both formats.
/// The 31-day maximum is an SDK scope bound, not a USGS service limitation.
#[derive(Clone, Debug)]
pub struct Window {
    start: Timestamp,
    end: Timestamp,
    magnitude: Option<Magnitude>,
    max_magnitude: Option<Magnitude>,
    size: PageSize,
    order_by: OrderBy,
}

impl Window {
    /// Validate two timestamps and the bounded interval. Equal instants are allowed.
    pub fn new(start: &str, end: &str) -> Result<Self, Error> {
        Self::between(Timestamp::parse(start)?, Timestamp::parse(end)?)
    }

    pub(crate) fn between(start: Timestamp, end: Timestamp) -> Result<Self, Error> {
        let span = end.0.signed_duration_since(start.0);
        if span < TimeDelta::zero() || span > TimeDelta::days(31) {
            return Err(Error::InvalidInput(
                "window must be ordered and at most 31 days",
            ));
        }
        Ok(Self {
            start,
            end,
            magnitude: None,
            max_magnitude: None,
            size: PageSize::DEFAULT,
            order_by: OrderBy::default(),
        })
    }

    /// Set the minimum magnitude for GeoJSON and CSV alike.
    /// Bound ordering is checked by `validate()` or generated `send()` before I/O.
    #[must_use]
    pub fn min_magnitude(mut self, value: Magnitude) -> Self {
        self.magnitude = Some(value);
        self
    }

    /// Set the maximum magnitude for GeoJSON and CSV alike.
    /// Bound ordering is checked by `validate()` or generated `send()` before I/O.
    #[must_use]
    pub fn max_magnitude(mut self, value: Magnitude) -> Self {
        self.max_magnitude = Some(value);
        self
    }

    /// Set the same provider ordering for GeoJSON and CSV, including later pages.
    #[must_use]
    pub fn with_order_by(mut self, value: OrderBy) -> Self {
        self.order_by = value;
        self
    }

    /// Check that the minimum magnitude does not exceed the maximum, if both are set.
    /// Equal bounds are allowed. Generated `send()` repeats this check after scope setters.
    pub fn validate(&self) -> Result<(), Error> {
        validate_magnitude_bounds(self.magnitude, self.max_magnitude)
    }

    /// Set the result limit for both output formats.
    #[must_use]
    pub fn with_page_size(mut self, size: PageSize) -> Self {
        self.size = size;
        self
    }

    /// Inclusive lower time bound.
    pub fn start(&self) -> Timestamp {
        self.start
    }
    /// Inclusive upper time bound.
    pub fn end(&self) -> Timestamp {
        self.end
    }
    /// Optional minimum magnitude.
    pub fn magnitude(&self) -> Option<Magnitude> {
        self.magnitude
    }
    /// Optional maximum magnitude.
    pub fn maximum_magnitude(&self) -> Option<Magnitude> {
        self.max_magnitude
    }
    /// Provider ordering, defaulting to `time-asc` for compatibility.
    pub fn order_by(&self) -> OrderBy {
        self.order_by
    }
    /// Per-response result limit.
    pub fn page_size(&self) -> PageSize {
        self.size
    }
}

/// An opaque provider event identifier. No URL is followed to retrieve it.
///
/// `Debug` withholds it: the event lookup sends it as a `#[sensitive]` query value. `Display`
/// shows it, for deliberate use.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
#[derive(concord::Debug)]
pub struct EventId(#[sensitive] String);

impl EventId {
    /// Accept a nonempty identifier of at most 256 bytes without control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(Error::InvalidInput("invalid event identifier"));
        }
        Ok(Self(value))
    }

    /// Provider text, unescaped.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EventId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
