//! Value-free lifecycle facts of one managed response stream.

use http::{Method, StatusCode};

use crate::endpoint::EndpointId;
use crate::error::DispatchKnowledge;
use crate::hooks::AttemptInfo;
use crate::operation::OperationId;

use super::BodyDisposition;

/// Lifecycle state of one managed [`ByteStream`](crate::contracts::ByteStream)
/// or `ItemStream` (with the `records` feature).
///
/// Acquisition is not end of body: a stream returned by an endpoint await is
/// `Active` until the caller observes the actual end of the body (`Eof`),
/// stops early through `finish` (`Application`), cancels it (`Cancelled`) or
/// receives its one terminal error (`Failed`). Terminal states never change
/// again and never regress to `Active`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StreamState {
    /// The stream is open and may still yield data.
    Active,
    /// The caller polled the stream through the actual end of the body.
    Eof,
    /// The application called `finish` before the end of the body. The body
    /// was not drained; this is not completion.
    Application,
    /// The stream was dropped before a terminal result, or an attached
    /// cancellation signal terminated it.
    Cancelled,
    /// The stream yielded its one terminal error (read failure, limit, lease
    /// or idle expiry, framing or decoding failure).
    Failed,
}

/// Structural kind of the one terminal error a managed stream yielded.
///
/// The kind carries no body bytes, record values, header values or parser
/// text. It refines the [`ErrorCategory`](crate::contracts::ErrorCategory) of
/// the yielded [`ApiError`](crate::contracts::ApiError).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StreamErrorKind {
    /// The native body read failed after the response head was accepted.
    Body,
    /// The cumulative decoded (post-decompression) nonempty byte limit would
    /// be exceeded; the first excess chunk was read and withheld.
    ByteLimit,
    /// More than the permitted number of consecutive empty transport chunks
    /// arrived.
    EmptyChunkLimit,
    /// The finite total lease (including any whole-operation timeout cap)
    /// elapsed. Consumer pauses between polls count toward this lease, and a
    /// read observed to complete at or after the lease end is withheld. When
    /// the idle budget has run out as well, this kind takes precedence over
    /// [`IdleTimeout`](Self::IdleTimeout).
    LeaseExpired,
    /// Cumulative idle time while an underlying read was pending reached the
    /// idle limit without new nonempty data. This includes a pending read
    /// whose `next` future was dropped and whose data or end of body became
    /// available only after the limit, and the time from a cooperative yield
    /// over empty chunks to the next poll.
    IdleTimeout,
    /// The record-count limit would be exceeded; the excess record was
    /// withheld.
    RecordLimit,
    /// One framed record would exceed the per-record staging ceiling.
    FrameLimit,
    /// Record boundaries or quoting were malformed.
    InvalidFraming,
    /// A complete framed record could not be decoded.
    Decode,
    /// The body ended inside an incomplete record.
    IncompleteRecord,
    /// An attached cancellation signal terminated the stream.
    Cancelled,
    /// A core or decoder contract invariant failed safely.
    Internal,
}

/// Owned value-free snapshot of one managed stream's lifecycle journal.
///
/// A summary is a snapshot, not a live view: taking it does not change the
/// stream, and it never mutates acquisition metadata returned earlier by the
/// endpoint (that metadata keeps reporting
/// [`BodyDisposition::StreamOpened`]). It contains no body bytes, record
/// values, URL, header values or credential material, so it is safe to log.
///
/// The correlation facts (endpoint, operation, attempt and status) are those
/// of the original request whose accepted response head opened the stream.
/// Dispatch knowledge is always
/// [`DispatchKnowledge::ResponseObserved`], because a stream exists only
/// after a response head was received.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamSummary {
    pub(crate) state: StreamState,
    pub(crate) endpoint: EndpointId,
    pub(crate) method: Method,
    pub(crate) operation_id: OperationId,
    pub(crate) attempt: AttemptInfo,
    pub(crate) status: StatusCode,
    pub(crate) acquired_bytes: u64,
    pub(crate) delivered_records: usize,
    pub(crate) body_disposition: BodyDisposition,
    pub(crate) error_kind: Option<StreamErrorKind>,
}

impl StreamSummary {
    /// Returns the lifecycle state at the time of the snapshot.
    #[must_use]
    pub const fn state(&self) -> StreamState {
        self.state
    }

    /// Returns the identity of the request operation that opened the stream.
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    /// Returns the visible attempt whose accepted response opened the stream.
    #[must_use]
    pub const fn attempt(&self) -> AttemptInfo {
        self.attempt
    }

    /// Returns the accepted response status of the original request.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the nonempty decoded body bytes read from the transport so far.
    ///
    /// A chunk counts once it was actually read, even when it was then
    /// withheld: the first excess chunk withheld by the byte limit, or a
    /// chunk whose read was observed to complete only after the total lease
    /// or the pending-read idle budget had run out. When a limit has already
    /// run out at the start of a poll, the stream terminates before reading
    /// again, so data that had merely become available is neither read nor
    /// counted. Empty chunks never count.
    #[must_use]
    pub const fn acquired_bytes(&self) -> u64 {
        self.acquired_bytes
    }

    /// Returns the number of complete records yielded to the caller.
    ///
    /// A `ByteStream` always reports zero; an `ItemStream` counts every full
    /// record it yielded (a withheld or failed record is not counted).
    #[must_use]
    pub const fn delivered_records(&self) -> usize {
        self.delivered_records
    }

    /// Returns what happened to the native response body so far.
    ///
    /// `StreamOpened` while active, `Complete` after the actual end of the
    /// body, `Failed` after a terminal error, and `Partial` (polled) or
    /// `DiscardedUnread` (never polled) after an early `finish` or
    /// cancellation. An early stop is never reported as `Complete`.
    #[must_use]
    pub const fn body_disposition(&self) -> BodyDisposition {
        self.body_disposition
    }

    /// Returns the aggregate dispatch knowledge; always `ResponseObserved`.
    #[must_use]
    pub const fn dispatch_knowledge(&self) -> DispatchKnowledge {
        DispatchKnowledge::ResponseObserved
    }

    /// Returns the kind of the one terminal error the stream yielded, if any.
    ///
    /// A stream that reached `Eof`, was finished by the application or was
    /// dropped yielded no error and reports `None`.
    #[must_use]
    pub const fn error_kind(&self) -> Option<StreamErrorKind> {
        self.error_kind
    }

    /// Returns the operational endpoint ID (an explicit `id` pin or navigation-derived) of the
    /// original request.
    #[must_use]
    pub const fn endpoint_id(&self) -> &EndpointId {
        &self.endpoint
    }

    /// Returns the application HTTP method of the original request.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }
}
