//! Incremental bounded record decoding over Concord's one byte-stream lifecycle.

mod decoder;
mod driver;
mod error;
mod stream;

pub use decoder::RecordDecoder;
pub use error::{RecordError, RecordErrorKind};
pub use stream::{RecordBatches, RecordStream, Records};
