//! Private operation timing and public opaque correlation identity.

mod deadline;
mod id;

pub(crate) use deadline::OperationDeadline;
#[cfg(test)]
pub(crate) use deadline::TestClockGuard;
pub use id::OperationId;
pub(crate) use id::OperationIdExhausted;
