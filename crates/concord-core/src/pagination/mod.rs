//! Stable pagination contracts and the single production sequential page loop.
//!
//! The retained pagination-identity byte limit bounds the complete canonical
//! encoded identity vectors used for cycle detection. The runtime page ceiling
//! separately bounds the number of retained identities; allocator and
//! container overhead is not included in the retained-byte total.

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
