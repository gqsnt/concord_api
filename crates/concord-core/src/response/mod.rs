//! Successful response metadata and built-in response markers.
//!
//! Ordinary complete buffered codecs implement
//! [`crate::codec::DecodeBody`]; its blanket implementation supplies
//! [`ResponseBodySpec`]. Implement `ResponseBodySpec` directly only to own the
//! full validation and conversion contract for a sealed response mode. There
//! is no public `ResponseBody` trait. [`ResponseMode::Body`] is the semantic
//! value acquired by core and passed to `ResponseBodySpec::from_body`, never a
//! native response.

mod error;
mod handler;
mod metadata;
mod spec;
mod stream;

pub use error::{ResponseBodyError, ResponseBodyErrorKind};
#[cfg(feature = "dangerous-raw-response")]
pub use metadata::RawResponse;
pub use metadata::{ApiResponse, ExecutionMeta};
pub use spec::{
    Buffered, Discard, NoContent, PageResponseSpec, ResponseBodySpec, ResponseHead, ResponseMode,
    Streaming,
};
pub use stream::ByteStream;

pub(crate) use handler::{ResponseInput, execute_response};
#[cfg(feature = "dangerous-raw-response")]
#[doc(hidden)]
pub use spec::RawResponseMode;
pub(crate) use spec::{completion_outcome, validate_accept};
