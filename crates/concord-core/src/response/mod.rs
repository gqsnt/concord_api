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
//! [`MAX_EMPTY_RESPONSE_CHUNKS`] for one response.

mod error;
mod handler;
mod metadata;
mod spec;
mod stream;

pub use crate::limits::MAX_EMPTY_RESPONSE_CHUNKS;
pub use error::{ResponseBodyError, ResponseBodyErrorKind};
pub use metadata::{ApiResponse, ExecutionMeta};
pub use spec::{
    AcceptedBytesDriver, Buffered, Discard, DiscardDriver, Empty, PageResponseRepresentation,
    ResponseDriver, ResponseHead, ResponseRepresentation, Streaming,
};
pub use stream::ByteStream;

pub(crate) use handler::{ResponseInput, execute_response};
pub(crate) use spec::{
    AcceptedBytesSelection, DeclaredResponse, ResponseSelection, completion_outcome,
    validate_accept,
};
pub(crate) use stream::StreamObservation;
