//! Page-number pagination advances a non-zero page value safely.
//!
//! Required feature: `json`. Mode: construction-only.
//! Documentation owner: `docs/guide/pagination.md`.
//! Proving test: `tests/construction.rs`.

use concord::{PageLimit, PaginatedRequest};

use super::PaginationApi;

/// Constructs a two-page page-number request.
pub fn construct(client: &PaginationApi) {
    let _: PaginatedRequest<_> = client
        .page(
            std::num::NonZeroU64::MIN,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
}
