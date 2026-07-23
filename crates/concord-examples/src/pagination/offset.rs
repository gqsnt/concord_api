//! Offset pagination binds its next offset and fixed page size.
//!
//! Required feature: `json`. Mode: deterministic local execution.
//! Documentation owner: `docs/guide/06-pagination-and-rate-limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::{PageLimit, PaginatedRequest};

use super::PaginationApi;

/// Constructs a two-page offset-pagination request.
pub fn construct(client: &PaginationApi) {
    let _: PaginatedRequest<_> = client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
}
