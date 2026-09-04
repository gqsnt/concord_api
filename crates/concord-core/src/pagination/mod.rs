//! Stable pagination contracts and the single production sequential page loop.
//!
//! The retained pagination-identity byte limit bounds the complete canonical
//! encoded identity vectors used for cycle detection. The runtime page ceiling
//! separately bounds the number of retained identities; allocator and
//! container overhead is not included in the retained-byte total. A separate
//! aggregate pagination response-body budget charges the exact buffered bytes
//! acquired for successful pages before their decoded items are retained.

mod controller;
mod error;
mod execute;
mod identity;
mod limits;
mod pending;

pub use crate::error::PaginationErrorKind;
pub use controller::{
    MAX_PAGINATION_HEADER_BYTES, MAX_PAGINATION_HEADER_INTERESTS, PageCorrelation,
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
};
pub use error::PaginationError;
pub use pending::{PageLimit, PaginatedEndpoint, PaginatedRequest};

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
