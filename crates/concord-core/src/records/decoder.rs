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
/// bound, shared by the supplied buffer and reported `buffered_bytes`.
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
///
/// # Extension contract
///
/// * **Ownership.** The decoder value created by `start` belongs to exactly
///   one response and is dropped with it; the staging buffer is core-owned
///   and lent for one callback at a time. Decoded records are moved to the
///   caller.
/// * **Concurrency.** A decoder is `Send` but never shared: one response
///   drives its callbacks sequentially. Decoders of independent responses may
///   run concurrently.
/// * **Cancellation.** Callbacks are synchronous and cannot be preempted.
///   They run while the caller polls, under the stream lease, idle timeout
///   and cancellation checks; dropping an item stream drops the decoder
///   without further calls.
/// * **Input sensitivity.** Inputs and decoder state may be sensitive;
///   implementations own logging, storage and metrics, and core never prints
///   staged bytes or records.
/// * **Errors.** A [`RecordError`] from any callback terminates decoding:
///   core keeps only its kind, releases the staged bytes and reports a
///   value-free failure as the final stream item (or the buffered result).
/// * **Resource obligations.** Core bounds acquired bytes, staged bytes
///   (`max_buffered_bytes`) and the record count; implementations own their
///   CPU, memory, metric cardinality and external I/O behavior, including the
///   heap size of each decoded record.
/// * **Negative capabilities.** A decoder cannot poll the body, change the
///   accepted status, extend a lease or observe the transport or native
///   response.
pub trait RecordDecoder: Sized + Send + 'static {
    /// One successfully decoded record.
    type Record: Send + 'static;

    /// Static `Accept` value negotiated by representations built on this
    /// decoder. Read without running decoder code.
    const ACCEPT: Option<&'static str> = None;

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

    /// Bytes retained by the decoder while assembling the current record.
    ///
    /// These count toward the same staging ceiling as the supplied buffer.
    /// Decoders that consume partial framing into owned storage must report
    /// that storage here so a small runtime frame limit remains a hard bound.
    fn buffered_bytes(&self) -> usize {
        0
    }

    /// Limits the next transfer from pending transport bytes into staging.
    ///
    /// A decoder that expands text while retaining partial records can stop
    /// at a framing boundary, avoiding staging unrelated later records beside
    /// the expanded text. Return a positive length no larger than `input.len()`
    /// when input is nonempty. The default stages all available input.
    fn input_bytes(&self, input: &[u8]) -> usize {
        input.len()
    }

    /// Returns the positive finite maximum number of undecoded staged bytes.
    fn max_buffered_bytes() -> NonZeroUsize;
}
