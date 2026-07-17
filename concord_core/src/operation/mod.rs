//! Private operation timing and public opaque correlation identity.

mod deadline;
mod id;

#[allow(
    unused_imports,
    reason = "wired into the production executor by PR 2.3"
)]
pub(crate) use deadline::{DeadlineElapsed, DeadlineOverflow, OperationDeadline};
pub use id::OperationId;
pub(crate) use id::OperationIdExhausted;
