//! Stable pagination contracts and the single production sequential page loop.

mod controller;
mod error;
mod execute;
mod identity;
mod limits;
mod pending;
mod request;

pub use controller::{
    CursorPagination, HasNextCursor, OffsetPagination, PagePagination, PaginationController,
};
pub use error::PaginationError;
pub use pending::{PageLimit, PaginatedEndpoint, PaginatedRequest};
pub use request::PaginationRequest;

#[doc(hidden)]
pub use execute::paginate_with;
pub(crate) use request::{HeaderPatch, PaginationPatch, QueryPatch};

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
