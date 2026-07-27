use std::num::NonZeroUsize;

use bytes::BytesMut;

use crate::response::ResponseHead;

use super::RecordError;

/// Incrementally frames and decodes typed records from bounded response bytes.
///
/// `validate` always runs synchronously after accepted status and known-length
/// preflight but before the first body poll. `start` creates one request-local
/// decoder before the first decoder callback. Streaming record responses also
/// call `start` before the first body poll; complete buffered record responses
/// may acquire their bounded body first. Decoder methods run without a core
/// lock and receive neither transport nor native-response access.
///
/// `decode` is called repeatedly while it can consume complete records before
/// another source chunk is requested; transport chunks are never record
/// boundaries and may exceed the staging limit when they contain smaller
/// logical records. `decode_eof` owns exact final framing semantics.
/// `max_buffered_bytes` is an independently enforced positive finite staging
/// bound. Inputs and decoder state may be sensitive; implementations own
/// logging, storage, CPU, memory, metric cardinality, and external I/O
/// behavior.
///
/// Core enforces net staging-buffer progress across each callback.
/// Implementations must not increase the supplied staging buffer's length.
/// Returning `Some(record)` requires a strict net decrease in buffer length.
/// Returning `None` may consume bytes; when it leaves the length unchanged,
/// core stages more pending source bytes, requests more body data, or completes
/// EOF handling. A decoder may mutate the buffer only within these resulting
/// net-progress constraints. Core terminates record decoding with an internal
/// record error when the observed before-and-after lengths violate the
/// contract.
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

    /// Validates the accepted response head before the first body poll.
    fn validate(_head: &ResponseHead<'_>) -> Result<(), RecordError> {
        Ok(())
    }

    /// Creates one request-local decoder before the first decoder callback.
    fn start(head: &ResponseHead<'_>) -> Result<Self, RecordError>;

    /// Decodes at most one record while obeying the trait-level progress contract.
    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;

    /// Resolves at most one final record under the same progress contract.
    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;

    /// Returns the positive finite maximum number of undecoded staged bytes.
    fn max_buffered_bytes() -> NonZeroUsize;
}
