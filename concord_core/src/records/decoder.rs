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

    /// Decodes at most one record from the current staging buffer.
    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;

    /// Resolves at most one final record after the byte stream reaches EOF.
    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;

    /// Returns the positive finite maximum number of undecoded staged bytes.
    fn max_buffered_bytes() -> NonZeroUsize;
}
