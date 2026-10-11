//! Bounded streaming and complete buffered record decoding through one driver.

mod buffered;
mod decoder;
mod driver;
mod error;
mod stream;

#[cfg(test)]
pub(crate) use buffered::BufferedRecords;
#[doc(hidden)]
pub use buffered::{decode_complete_records, validate_record_decoder};
pub use decoder::RecordDecoder;
pub use error::{RecordError, RecordErrorKind};
pub use stream::{ItemBatches, ItemStream};
#[cfg(test)]
pub(crate) use stream::{RecordStream, Records};
