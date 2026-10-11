//! Offset pagination binds its next offset and fixed page size.
//!
//! Required feature: `json`. Mode: deterministic local execution.
//! Documentation owner: `docs/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use std::num::NonZeroU32;

use concord::pagination::{CollectPrefix, Prefix};

use super::PaginationApi;

/// Constructs a two-page offset-pagination prefix request.
// ANCHOR: offset_pagination
pub fn construct(client: &PaginationApi) {
    let _: CollectPrefix<u32> = client
        .offset(0, NonZeroU32::new(50).expect("positive page size"))
        .collect_prefix(2);
}
// ANCHOR_END: offset_pagination

/// Executes a bounded offset traversal and returns an explicit prefix.
///
/// The [`Prefix`] summary distinguishes provider exhaustion from the page
/// bound; the items are never an undisclosed partial result.
// ANCHOR: execute_pagination
pub async fn collect(client: &PaginationApi) -> Result<Prefix<u32>, concord::ApiError> {
    client
        .offset(0, NonZeroU32::new(50).expect("positive page size"))
        .collect_prefix(2)
        .await
}
// ANCHOR_END: execute_pagination

/// Collects until natural termination with a finite fail-closed cap.
///
/// Reaching the bound while the controller still offers continuation is a
/// pagination completion-limit error, never a success vector.
// ANCHOR: paginate_all
pub async fn collect_all(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(0, NonZeroU32::new(50).expect("positive page size"))
        .collect_all(NonZeroU32::new(100).expect("positive completion limit"))
        .await
}
// ANCHOR_END: paginate_all
