//! Successful response metadata and built-in response markers.

mod handler;
mod metadata;
mod stream;

pub use metadata::{DecodedResponse, ExecutionMeta};
pub use stream::StreamResponse;

pub(crate) use handler::validate_stream_media_marker;
#[doc(hidden)]
pub use handler::{ExecuteResponse, ResponseInput};

/// DSL marker for a response whose successful body is intentionally ignored.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContent;
