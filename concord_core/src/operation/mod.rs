//! Private operation timing and public opaque correlation identity.

mod deadline;
mod id;

pub(crate) use deadline::OperationDeadline;
pub use id::OperationId;
pub(crate) use id::OperationIdExhausted;
