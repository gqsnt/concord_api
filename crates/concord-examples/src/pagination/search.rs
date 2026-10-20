//! POST pagination repeats one trusted, page-stable JSON body.
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! multi-page execution. Documentation owner: `docs/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! The endpoint declares `effect safe` (the provider's assertion for this
//! search) and `Json` opts into the page-stable body contract for `Clone`
//! inputs, so each page prepares the same captured filter.

use std::num::NonZeroU32;

use concord::pagination::CollectPrefix;
use serde::Serialize;

use super::PaginationApi;

/// Stable search criteria sent with every page request.
#[derive(Clone, Debug, Serialize)]
pub struct SearchFilter {
    /// Full-text query.
    pub query: String,
    /// Whether archived records match.
    pub include_archived: bool,
}

/// Constructs a bounded POST prefix request with a repeatable JSON body.
// ANCHOR: post_search_request
pub fn construct(client: &PaginationApi) {
    let filter = SearchFilter {
        query: "rust".to_owned(),
        include_archived: false,
    };
    let _: CollectPrefix<u32> = client
        .search(0, NonZeroU32::new(50).expect("positive page size"), filter)
        .collect_prefix(2);
}
// ANCHOR_END: post_search_request
