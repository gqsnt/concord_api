//! Page-number pagination advances a non-zero page value safely.
//!
//! Required feature: `json`. Mode: construction-only.
//! Documentation owner: `docs/pagination.md`.
//! Proving test: `tests/construction.rs`.

use std::num::{NonZeroU32, NonZeroU64};

use concord::pagination::CollectPrefix;

use super::PaginationApi;

/// Constructs a two-page page-number prefix request.
pub fn construct(client: &PaginationApi) {
    let _: CollectPrefix<u32> = client
        .page(
            NonZeroU64::MIN,
            NonZeroU32::new(50).expect("positive page size"),
        )
        .collect_prefix(2);
}
