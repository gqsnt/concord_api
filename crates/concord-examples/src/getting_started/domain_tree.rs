//! A domain-shaped client with optional values, defaults, planning, and pagination.
//!
//! Required feature: `json`. Mode: construction-only. Documentation owners:
//! `README.md`, `docs/quick_start.md`, `docs/overview.md`, and
//! `docs/mental_model.md`.
//! Proving test: `tests/construction.rs`.
//!
//! The base is `http` because this tutorial performs no live transport and must
//! build under `json` alone. An `https` base additionally requires a TLS
//! capability such as `default-tls` and otherwise fails with `TlsUnavailable`.

// ANCHOR: domain_tree_types
use std::error::Error;
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
        response Json;

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
            ) -> Vec<ItemSummary> {
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
    }
}
// ANCHOR_END: domain_tree_contract

/// Builds the immutable client snapshot without performing I/O.
// ANCHOR: domain_tree_builder
pub fn build_client() -> Result<CatalogApi, concord::ClientBuildError> {
    CatalogApi::builder().set_tenant("acme".to_owned()).build()
}
// ANCHOR_END: domain_tree_builder

/// Constructs one ordinary request without executing it.
// ANCHOR: domain_tree_ordinary_request
pub fn ordinary_request(client: &CatalogApi) {
    let _request = client.regional("eu".to_owned()).item_summaries();
}
// ANCHOR_END: domain_tree_ordinary_request

/// Demonstrates the exact optional and defaulted controls.
// ANCHOR: domain_tree_controls
pub fn controlled_request(client: &CatalogApi) {
    let _request = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .omit_category()
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .reset_count();
}
// ANCHOR_END: domain_tree_controls

/// Constructs a bounded pagination terminal without polling it.
// ANCHOR: domain_tree_pagination
pub fn paginated_request(client: &CatalogApi) {
    let _: PaginatedRequest<_> = client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .paginate(PageLimit::Prefix(2));
}
// ANCHOR_END: domain_tree_pagination

/// Inspects an unbound endpoint and both plan entry points without execution.
// ANCHOR: domain_tree_planning
pub fn inspect(client: &CatalogApi) -> Result<(), concord::ApiError> {
    let endpoint = CatalogApi::unbound()
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()));
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _bound_plan = request.plan()?;
    Ok(())
}
// ANCHOR_END: domain_tree_planning

/// Compile-checks the complete construction-only tutorial as one program.
pub fn construct() -> Result<(), Box<dyn Error>> {
    let client = build_client()?;
    ordinary_request(&client);
    controlled_request(&client);
    paginated_request(&client);
    inspect(&client)?;
    Ok(())
}

/// Awaits one ordinary request.
///
/// This function is compile-checked but never executed: the tutorial declares
/// an example host and performs no live transport.
// ANCHOR: domain_tree_await_request
pub async fn fetch_one_page(client: &CatalogApi) -> Result<Vec<ItemSummary>, concord::ApiError> {
    client
        .regional("eu".to_owned())
        .item_summaries()
        .set_category(Some("hardware".to_owned()))
        .await
}
// ANCHOR_END: domain_tree_await_request

/// Awaits at most two pages as one complete vector.
// ANCHOR: domain_tree_await_pagination
pub async fn fetch_two_pages(client: &CatalogApi) -> Result<Vec<ItemSummary>, concord::ApiError> {
    client
        .regional("eu".to_owned())
        .item_summaries()
        .paginate(PageLimit::Prefix(2))
        .await
}
// ANCHOR_END: domain_tree_await_pagination
