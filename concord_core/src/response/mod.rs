//! Successful response metadata and built-in response markers.

mod handler;
mod metadata;

pub use metadata::{DecodedResponse, ExecutionMeta};

pub(crate) use handler::{ExecuteResponse, ResponseInput};

/// DSL marker for a response whose successful body is intentionally ignored.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContent;
