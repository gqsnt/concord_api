//! Accepted response metadata and built-in response markers.
//!
//! Ordinary complete buffered codecs implement
//! [`crate::codec::DecodeBody`]; its blanket implementation supplies
//! [`ResponseSpec`]. Implement `ResponseSpec` directly only to own the
//! full validation and conversion contract for a sealed response mode. There
//! is no public `ResponseBody` trait. [`ResponseMode::Body`] is the semantic
//! value acquired by core and passed to `ResponseSpec::from_body`, never a
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
    AcceptedBytesMode, Buffered, Discard, NoContent, PageResponseSpec, ResponseHead, ResponseMode,
    ResponseSpec, Streaming,
};
pub use stream::ByteStream;

pub(crate) use handler::{ResponseInput, execute_response};
pub(crate) use spec::{
    AcceptedBytesSelection, DeclaredResponse, ResponseSelection, completion_outcome,
    validate_accept,
};
