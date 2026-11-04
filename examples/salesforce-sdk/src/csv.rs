//! The Salesforce Bulk API 2.0 CSV dialect on top of `concord::csv` (in progress).

use std::fmt;

/// A CSV configuration or encoding failure before anything was sent.
#[derive(Debug)]
#[non_exhaustive]
pub enum CsvError {
    /// Placeholder until the codec lands.
    Pending,
    /// An upload body over the limit, refused before sending.
    UploadTooLarge {
        /// The limit in bytes.
        limit: usize,
    },
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("csv")
    }
}

impl std::error::Error for CsvError {}
