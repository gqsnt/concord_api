//! Concrete ecosystem-backed formats for buffered and streaming records.
//!
//! With the `csv` feature, `Csv` and its sealed header and delimiter policies
//! provide compile-time CSV dialect selection without runtime configuration.

#[cfg(feature = "csv")]
mod csv;
#[cfg(feature = "sse")]
mod event_source;
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
pub use event_source::EventSource;
#[cfg(feature = "sse")]
pub use sse::{EventStream, ServerEvent};
