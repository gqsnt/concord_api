use std::num::NonZeroUsize;

use bytes::BytesMut;

use crate::response::ResponseHead;

use super::RecordError;

/// Incrementally frames and decodes typed records from bounded response bytes.
///
/// `validate` and `start` run synchronously after successful status and
/// streaming known-length preflight but before the first body poll. Concord
/// creates one decoder per returned record stream. Decoder methods run without
/// a core lock and receive neither transport nor native-response access.
///
/// `decode` is called repeatedly while it can consume complete records before
/// another network chunk is polled; network chunks are never record boundaries.
/// `decode_eof` owns exact final framing semantics. `max_buffered_bytes` is an
/// independently enforced positive finite staging bound. Inputs and decoder
/// state may be sensitive; implementations own logging, storage, CPU, memory,
/// metric cardinality, and external I/O behavior.
///
/// Implementations must not append to the supplied staging buffer. Returning
/// `Some(record)` requires removing at least one byte from the buffer. Core
/// terminates the stream with an internal record error when this contract is
/// violated. `decode` may consume bytes and return `None`; core immediately
/// retries decoding before polling the response body. `decode_eof` follows the
/// same progress rules after the body reaches EOF.
///
/// One logical decoder output must correspond to staged-byte consumption. A
/// format that expands one framed input into several values should return that
/// collection as one record or use a different response specification.
pub trait RecordDecoder: Sized + Send + 'static {
    /// One successfully decoded record.
    type Record: Send + 'static;

    /// Default `Accept` header inserted only when the request has none.
    fn accept() -> Option<&'static str> {
        None
    }

    /// Validates the successful response head before the first body poll.
    fn validate(_head: &ResponseHead<'_>) -> Result<(), RecordError> {
        Ok(())
    }

    /// Creates one request-local decoder before the first body poll.
    fn start(head: &ResponseHead<'_>) -> Result<Self, RecordError>;

    /// Decodes at most one record while obeying the trait-level progress contract.
    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;

    /// Resolves at most one final record under the same progress contract.
    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;

    /// Returns the positive finite maximum number of undecoded staged bytes.
    fn max_buffered_bytes() -> NonZeroUsize;
}
