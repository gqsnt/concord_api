//! Stable pagination contracts and the single production sequential page loop.

mod controller;
mod error;
mod execute;
mod identity;
mod limits;
mod pending;

pub use crate::error::PaginationErrorKind;
pub use controller::PaginationController;
pub use error::PaginationError;
pub use pending::{PageLimit, PaginatedEndpoint, PaginatedRequest};

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
