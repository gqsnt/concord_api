//! Concrete ecosystem-backed formats for buffered and streaming records.
//!
//! With the `csv` feature, `Csv` and its sealed header and delimiter policies
//! provide compile-time CSV dialect selection without runtime configuration.
//! With `sse`, `EventStream` decodes response-only server-sent events.

#[cfg(feature = "csv")]
mod csv;
#[cfg(feature = "ndjson")]
mod ndjson;
#[cfg(feature = "sse")]
mod sse;

#[cfg(feature = "csv")]
pub use csv::{
    CommaDelimiter, Csv, CsvDelimiter, CsvHeaderMode, HasHeader, NoHeader, SemicolonDelimiter,
    TabDelimiter,
};
#[cfg(feature = "ndjson")]
pub use ndjson::NdJson;

#[cfg(feature = "sse")]
pub use sse::{EventStream, ServerEvent};
