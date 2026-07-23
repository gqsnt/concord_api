//! The extras cursor controller derives continuation from the decoded page.
//!
//! Required feature: `json`. Mode: construction and deterministic local proof.
//! Documentation owner: `docs/guide/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::{PageLimit, PaginatedRequest};

use super::PaginationApi;

/// Constructs a bounded cursor-pagination request.
pub fn construct(client: &PaginationApi) {
    let _: PaginatedRequest<_> = client.cursor().paginate(PageLimit::Take(2));
}
