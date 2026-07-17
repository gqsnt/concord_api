//! Successful response metadata and built-in response markers.

mod metadata;

pub use metadata::{DecodedResponse, ExecutionMeta};

/// DSL marker for a response whose successful body is intentionally ignored.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContent;
