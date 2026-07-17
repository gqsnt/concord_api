//! Final response-decoder contracts used by bounded terminal status bodies.

use std::error::Error;
use std::fmt;

use bytes::Bytes;
use http::{Method, StatusCode};

/// A safe failure reported by an application codec.
///
/// Codec-provided text is deliberately not reproduced by Concord formatting.
pub struct CodecError {
    _message: String,
}

impl CodecError {
    /// Creates a codec failure.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            _message: message.into(),
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("codec operation failed")
    }
}

impl fmt::Debug for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("CodecError").finish_non_exhaustive()
    }
}

impl Error for CodecError {}

/// Safe response metadata supplied to a response decoder.
pub struct DecodeContext<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'static str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Final application response status.
    pub status: StatusCode,
    /// Parsed response content type, when one was available.
    pub content_type: Option<&'a str>,
}

/// Decodes a complete bounded response body.
pub trait DecodeBody: Send + Sync + 'static {
    /// Value produced by the decoder.
    type Output: Send + 'static;

    /// Default response media range requested by this decoder.
    const ACCEPT: Option<&'static str>;

    /// Validates response media metadata before decoding.
    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    /// Decodes one complete bounded body.
    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError>;
}
