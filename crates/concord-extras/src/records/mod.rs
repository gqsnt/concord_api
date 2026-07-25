//! Concrete ecosystem-backed decoders for core's bounded record stream.
//!
//! With the `csv` feature, `Csv` and its sealed header and delimiter policies
//! provide compile-time CSV dialect selection without runtime configuration.

#[cfg(feature = "csv")]
mod csv;
#[cfg(feature = "ndjson")]
mod ndjson;

#[cfg(feature = "csv")]
pub use csv::{
    CommaDelimiter, Csv, CsvDelimiter, CsvHeaderMode, HasHeader, NoHeader, SemicolonDelimiter,
    TabDelimiter,
};
#[cfg(feature = "ndjson")]
pub use ndjson::NdJson;
