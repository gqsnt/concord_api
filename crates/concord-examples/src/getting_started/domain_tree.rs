//! A domain-shaped client with optional values, defaults, and pagination.
//!
//! Required feature: `json`. Mode: construction-only. Documentation owners:
//! `README.md`, `docs/quick_start.md`, and `docs/mental_model.md`.
//! Proving test: `tests/construction.rs`.
//!
//! The base is `http` because this tutorial performs no live transport and must
//! build under `json` alone. An `https` base additionally requires a TLS
//! capability such as `default-tls` and otherwise fails with `TlsUnavailable`.

// ANCHOR: domain_tree_types
use std::num::NonZeroU32;

use concord::pagination::PaginatedRequest;
use concord::{Json, PageLimit, api};
use serde::Deserialize;

/// Default page size retained by each endpoint snapshot.
pub const DEFAULT_PAGE_SIZE: NonZeroU32 = match NonZeroU32::new(25) {
    Some(value) => value,
    None => unreachable!(),
};

/// One item returned by the example catalog API.
#[derive(Debug, Deserialize)]
pub struct ItemSummary {
    /// Stable item identifier.
    pub id: String,
    /// Human-readable title.
    pub title: String,
}
// ANCHOR_END: domain_tree_types

// ANCHOR: domain_tree_contract
api! {
    client CatalogApi {
        timeout 30s;
        base "http://catalog.example.test/";
        values {
            tenant: String;
        }
        path ["api"];
        query { "tenant" = client.tenant; }


    scope regional(region: String) {
        subdomain replace [regional.region, "api"];
        path ["v1", "catalog"];

        GET item_summaries(
            category?: String,
            offset: u64 = 0,
            count: std::num::NonZeroU32 = DEFAULT_PAGE_SIZE,
        ) -> Vec<ItemSummary> via Json {
            path ["items"];
            query {
                "category" = endpoint.category;
                "offset" = endpoint.offset;
                "count" = endpoint.count;
            }
            paginate via concord::pagination::OffsetPagination {
                offset -> endpoint.offset;
                page_size -> endpoint.count;
            }
        }
    }
}}
// ANCHOR_END: domain_tree_contract

/// Builds the example client and constructs a bounded paginated request.
// ANCHOR: domain_tree_usage
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let client = CatalogApi::builder()
        .set_tenant("acme".to_owned())
        .build()?;

    let _: PaginatedRequest<_> = client
        .regional("eu".to_owned())
        .item_summaries()
        .with_category("hardware".to_owned())
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
    Ok(())
}
// ANCHOR_END: domain_tree_usage

/// Awaits one page and then a bounded two-page sequence.
///
/// This function is compile-checked but never executed: the tutorial declares
/// an example host and performs no live transport.
// ANCHOR: domain_tree_execution
pub async fn execute(client: &CatalogApi) -> Result<(usize, usize), concord::ApiError> {
    let one_page: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .with_category("hardware".to_owned())
        .await?;

    let first_two_pages: Vec<ItemSummary> = client
        .regional("eu".to_owned())
        .item_summaries()
        .paginate(PageLimit::Prefix(2))
        .await?;

    Ok((one_page.len(), first_two_pages.len()))
}
// ANCHOR_END: domain_tree_execution
