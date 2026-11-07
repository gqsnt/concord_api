use crate::EventId;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
enum CollectionTag {
    FeatureCollection,
}
#[derive(Clone, Debug, Deserialize)]
enum FeatureTag {
    Feature,
}
#[derive(Clone, Debug, Deserialize)]
enum PointTag {
    Point,
}

/// A GeoJSON result page. Unknown provider fields are tolerated.
#[derive(Clone, Debug, Deserialize)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    _kind: CollectionTag,
    /// Page metadata (count is not a stable total across pages).
    pub metadata: Metadata,
    /// Events in this page.
    pub features: Vec<Feature>,
}

/// The subset of GeoJSON page metadata used by this SDK.
#[derive(Clone, Debug, Deserialize)]
pub struct Metadata {
    /// Generation time in Unix milliseconds.
    pub generated: i64,
    /// Number of features in this response.
    pub count: usize,
    /// Provider status field; HTTP status remains authoritative.
    pub status: u16,
}

/// One event, used for list entries and event lookup.
#[derive(Clone, Debug, Deserialize)]
pub struct Feature {
    #[serde(rename = "type")]
    _kind: FeatureTag,
    /// Opaque ID used by `catalog.event(id)`.
    pub id: EventId,
    /// Event attributes; scientific interpretation is outside this slice.
    pub properties: Properties,
    /// Location when the provider supplies it.
    pub geometry: Option<Point>,
}

/// A GeoJSON Point: longitude, latitude, depth in kilometers.
#[derive(Clone, Debug, Deserialize)]
pub struct Point {
    #[serde(rename = "type")]
    _kind: PointTag,
    /// Longitude, latitude, depth, in that order.
    pub coordinates: [f64; 3],
}

/// Common event fields; nullable values remain optional.
#[derive(Clone, Debug, Deserialize)]
pub struct Properties {
    /// Preferred magnitude, possibly unknown.
    pub mag: Option<f64>,
    /// Human-readable location, possibly unknown.
    pub place: Option<String>,
    /// Origin time in Unix milliseconds.
    pub time: i64,
    /// Last update time in Unix milliseconds.
    pub updated: i64,
    /// Provider review/deletion status.
    pub status: String,
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Optional detail URL, retained as data and never followed by this SDK.
    pub detail: Option<String>,
    /// Detail products retained as JSON without modeling or fetching their contents.
    pub products: Option<serde_json::Value>,
}

/// A typed CSV row, mapped by header names. Empty optional numbers become `None`;
/// malformed numbers are errors, never defaulted or skipped. Extra columns are ignored.
#[derive(Clone, Debug, Deserialize)]
pub struct CsvRow {
    /// Provider timestamp text.
    pub time: String,
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// Depth in kilometers.
    pub depth: f64,
    /// Preferred magnitude.
    pub mag: Option<f64>,
    /// Magnitude method.
    #[serde(rename = "magType")]
    pub mag_type: String,
    /// Station count, when present.
    pub nst: Option<u32>,
    /// Azimuthal gap, when present.
    pub gap: Option<f64>,
    /// Minimum station distance, when present.
    pub dmin: Option<f64>,
    /// Root mean square travel-time residual, when present.
    pub rms: Option<f64>,
    /// Network code.
    pub net: String,
    /// Opaque event ID.
    pub id: EventId,
    /// Provider update timestamp text.
    pub updated: String,
    /// Location text, including literal commas, quotes and newlines.
    pub place: String,
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Horizontal location uncertainty.
    #[serde(rename = "horizontalError")]
    pub horizontal_error: Option<f64>,
    /// Depth uncertainty.
    #[serde(rename = "depthError")]
    pub depth_error: Option<f64>,
    /// Magnitude uncertainty.
    #[serde(rename = "magError")]
    pub mag_error: Option<f64>,
    /// Stations used for magnitude.
    #[serde(rename = "magNst")]
    pub mag_nst: Option<u32>,
    /// Provider review/deletion status.
    pub status: String,
    /// Location contributor.
    #[serde(rename = "locationSource")]
    pub location_source: String,
    /// Magnitude contributor.
    #[serde(rename = "magSource")]
    pub mag_source: String,
}
