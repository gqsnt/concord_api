//! POST pagination repeats one trusted, page-stable JSON body.
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! multi-page execution. Documentation owner: `docs/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::PageLimit;
use concord::pagination::PaginatedRequest;
use serde::Serialize;

use super::PaginationApi;

/// Stable search criteria sent with every page request.
#[derive(Clone, Debug, Serialize)]
pub struct SearchFilter {
    pub query: String,
    pub include_archived: bool,
}

/// Constructs a bounded POST pagination request with a repeatable JSON body.
// ANCHOR: post_search_request
pub fn construct(client: &PaginationApi) {
    let filter = SearchFilter {
        query: "rust".to_owned(),
        include_archived: false,
    };
    let _: PaginatedRequest<_> = client
        .search(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
            filter,
        )
        .paginate(PageLimit::Take(2));
}
// ANCHOR_END: post_search_request
