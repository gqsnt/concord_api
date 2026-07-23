//! Concrete ecosystem-backed decoders for core's bounded record stream.

#[cfg(feature = "csv")]
mod csv;
#[cfg(feature = "ndjson")]
mod ndjson;

#[cfg(feature = "csv")]
pub use csv::Csv;
#[cfg(feature = "ndjson")]
pub use ndjson::NdJson;
