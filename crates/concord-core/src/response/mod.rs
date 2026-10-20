//! Accepted response metadata and built-in response markers.
//!
//! Ordinary complete buffered codecs implement
//! [`crate::codec::DecodeBody`]; its blanket implementation supplies
//! [`ResponseRepresentation`]. Implement `ResponseRepresentation` directly only to own the
//! full validation and conversion contract for a sealed response mode. There
//! is no public `ResponseBody` trait. [`ResponseDriver::Body`] is the semantic
//! value acquired by core and passed to `ResponseRepresentation::from_body`, never a
//! native response. Every core-owned mode that polls a response body suppresses
//! empty transport chunks and accepts at most
//! [`MAX_EMPTY_RESPONSE_CHUNKS`] empty chunks (consecutive ones for a managed
//! stream).
//!
//! Streaming acquisition is distinct from consumption: an endpoint await that
//! returns a [`ByteStream`] (or an item stream) has opened a managed stream
//! with a finite lease, and the stream's [`StreamSummary`] reports how its
//! consumption ends.

mod absence;
mod error;
mod handler;
mod lease;
mod metadata;
mod spec;
mod stream;
mod summary;

pub use crate::limits::MAX_EMPTY_RESPONSE_CHUNKS;
pub use absence::OptionalOutcome;
pub use error::{ResponseBodyError, ResponseBodyErrorKind};
pub use metadata::{ApiResponse, BodyDisposition, ExecutionMeta};
pub use spec::{
    AcceptedBytesDriver, Buffered, Discard, DiscardDriver, Empty, MAX_ACCEPT_BYTES, ResponseDriver,
    ResponseHead, ResponseRepresentation, Streaming,
};
pub use stream::ByteStream;
pub use summary::{StreamErrorKind, StreamState, StreamSummary};

#[doc(hidden)]
pub use absence::{ExpectedAbsence, HeadAbsence};

pub(crate) use absence::{absent_outcome, present_outcome};
pub(crate) use handler::{ResponseInput, execute_response};
pub(crate) use lease::{StreamContext, StreamLimits};
pub(crate) use spec::{
    AcceptedBytesSelection, DeclaredResponse, ResponseSelection, accept_is_valid, body_disposition,
    completion_outcome, validate_accept,
};
#[cfg(feature = "records")]
pub(crate) use stream::ChunkEvent;
#[cfg(all(test, feature = "records"))]
pub(crate) use stream::test_support as stream_test_support;
