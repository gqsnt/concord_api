//! The extras cursor controller derives continuation from the decoded page.
//!
//! Required feature: `json`. Mode: construction and deterministic local execution.
//! Documentation owner: `docs/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::pagination::CollectPrefix;

use super::PaginationApi;

/// Constructs a bounded cursor-pagination prefix request.
pub fn construct(client: &PaginationApi) {
    let _: CollectPrefix<u32> = client.cursor().collect_prefix(2);
}
