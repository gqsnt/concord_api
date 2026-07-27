use std::error::Error;
use std::fmt;

/// A safe failure produced while opening or decoding a record response.
///
/// Built-in decoders discard parser-authored text and never attach record bytes,
/// response headers, filenames, JSON values, or CSV fields as sources. A
/// downstream decoder may add only application-approved safe text through
/// [`RecordError::with_safe_message`].
pub struct RecordError {
    kind: RecordErrorKind,
    safe_message: Option<String>,
}

impl RecordError {
    /// Creates a record failure with structural formatting only.
    #[must_use]
    pub const fn new(kind: RecordErrorKind) -> Self {
        Self {
            kind,
            safe_message: None,
        }
    }

    /// Creates a record failure with application-approved safe text.
    ///
    /// The caller is responsible for excluding record bytes, parser text,
    /// response metadata, filenames, fields, values, and other protected data.
    #[must_use]
    pub fn with_safe_message(kind: RecordErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: Some(message.into()),
        }
    }

    /// Returns the stable structural failure kind.
    #[must_use]
    pub const fn kind(&self) -> RecordErrorKind {
        self.kind
    }
}

impl fmt::Display for RecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("record operation failed")?;
        if let Some(message) = &self.safe_message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for RecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordError")
            .field("kind", &self.kind)
            .field("safe_message", &self.safe_message)
            .finish()
    }
}

impl Error for RecordError {}

/// Stable structural kinds of record-decoding failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordErrorKind {
    /// Accepted response media metadata violated the declared contract.
    InvalidMediaType,
    /// Record boundaries or quoting were malformed.
    InvalidFraming,
    /// A complete framed record could not be decoded safely.
    Decode,
    /// The final buffered bytes did not form a complete record.
    IncompleteRecord,
    /// The independent undecoded-record staging limit was exceeded.
    LimitExceeded,
    /// The underlying bounded response body or byte stream failed.
    Body,
    /// Concord could not classify an internal record failure more narrowly.
    Internal,
}
