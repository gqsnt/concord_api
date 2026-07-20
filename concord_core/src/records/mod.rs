//! Incremental bounded record decoding over Concord's one byte-stream lifecycle.

mod decoder;
mod error;
mod stream;

#[cfg(feature = "csv")]
mod csv;
#[cfg(feature = "ndjson")]
mod ndjson;

pub use decoder::RecordDecoder;
pub use error::{RecordError, RecordErrorKind};
pub use stream::{RecordBatches, RecordStream, Records};

#[cfg(feature = "csv")]
pub use csv::Csv;
#[cfg(feature = "ndjson")]
pub use ndjson::NdJson;
