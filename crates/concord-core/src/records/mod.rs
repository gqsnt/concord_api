//! Bounded streaming and complete buffered record decoding through one driver.

mod buffered;
mod decoder;
mod driver;
mod error;
mod stream;

pub use buffered::BufferedRecords;
pub use decoder::RecordDecoder;
pub use error::{RecordError, RecordErrorKind};
pub use stream::{RecordBatches, RecordStream, Records};
