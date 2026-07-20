//! Successful response metadata and built-in response markers.

mod error;
mod handler;
mod metadata;
mod spec;
mod stream;

pub use error::{ResponseBodyError, ResponseBodyErrorKind};
#[cfg(feature = "dangerous-raw-response")]
pub use metadata::RawResponse;
pub use metadata::{ApiResponse, DecodedResponse, ExecutionMeta};
pub use spec::{
    Buffered, Discard, NoContent, PageResponseSpec, ResponseBodySpec, ResponseHead, ResponseMode,
    Streaming,
};
pub use stream::ByteStream;

pub(crate) use handler::{ResponseInput, execute_response};
#[cfg(feature = "dangerous-raw-response")]
pub(crate) use spec::RawResponseMode;
pub(crate) use spec::{completion_outcome, validate_accept};
