//! Offset pagination binds its next offset and fixed page size.
//!
//! Required feature: `json`. Mode: deterministic local execution.
//! Documentation owner: `docs/features/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::PageLimit;
use concord::pagination::PaginatedRequest;

use super::PaginationApi;

/// Constructs a two-page offset-pagination request.
// ANCHOR: offset_pagination
pub fn construct(client: &PaginationApi) {
    let _: PaginatedRequest<_> = client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
}
// ANCHOR_END: offset_pagination

/// Executes a bounded offset-pagination operation and collects every item.
// ANCHOR: execute_pagination
pub async fn collect(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2))
        .await
}
// ANCHOR_END: execute_pagination

/// Collects until natural termination with a finite fail-closed cap.
// ANCHOR: paginate_all
pub async fn collect_all(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::HardCap(
            std::num::NonZeroU32::new(100).expect("positive hard cap"),
        ))
        .await
}
// ANCHOR_END: paginate_all
