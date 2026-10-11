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

use concord::pagination::{OffsetInit, OffsetPagination, Prefix};
use concord::{Json, api};
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
        base "http://catalog.example.test/";
        timeout 30s;
        response Json;

        values {
            query tenant: String;
        }

        path "api";

        scope regional(region: String) {
            subdomain replace [region, "api"];
            path "v1/catalog";

            GET item_summaries(
                query category?: String,
                query offset: u64 = 0,
                query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
            ) -> Vec<ItemSummary> {
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }
        }
    }
}
// ANCHOR_END: domain_tree_contract

/// Builds the immutable client snapshot without performing I/O.
// ANCHOR: domain_tree_builder
pub fn build_client() -> Result<CatalogApi, concord::ClientBuildError> {
    CatalogApi::builder().set_tenant("acme").build()
}
// ANCHOR_END: domain_tree_builder

/// Constructs one ordinary request without executing it.
// ANCHOR: domain_tree_ordinary_request
pub fn ordinary_request(client: &CatalogApi) {
    let _request = client.regional("eu").item_summaries();
}
// ANCHOR_END: domain_tree_ordinary_request

/// Demonstrates the exact optional and defaulted controls.
///
/// `set_category` sets the payload; `omit_category` removes it; `set_count`
/// replaces the initialized page size and `reset_count` restores the retained
/// declaration default without evaluating it again.
// ANCHOR: domain_tree_controls
pub fn controlled_request(client: &CatalogApi) {
    let _request = client
        .regional("eu")
        .item_summaries()
        .set_category("hardware")
        .omit_category()
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .reset_count();
}
// ANCHOR_END: domain_tree_controls

/// Constructs a bounded prefix-collection terminal without polling it.
// ANCHOR: domain_tree_pagination
pub fn paginated_request(client: &CatalogApi) {
    let _: concord::pagination::CollectPrefix<ItemSummary> = client
        .regional("eu")
        .item_summaries()
        .set_category("hardware")
        .set_count(NonZeroU32::new(50).expect("positive page size"))
        .collect_prefix(2);
}
// ANCHOR_END: domain_tree_pagination

/// Inspects an unbound endpoint and both plan entry points without execution.
// ANCHOR: domain_tree_planning
pub fn inspect(client: &CatalogApi) -> Result<(), concord::ApiError> {
    let endpoint = CatalogApi::unbound()
        .regional("eu")
        .item_summaries()
        .set_category("hardware");
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

/// Awaits one ordinary request. Ordinary `await` never paginates.
///
/// This function is compile-checked but never executed: the tutorial declares
/// an example host and performs no live transport.
// ANCHOR: domain_tree_await_request
pub async fn fetch_one_page(client: &CatalogApi) -> Result<Vec<ItemSummary>, concord::ApiError> {
    client
        .regional("eu")
        .item_summaries()
        .set_category("hardware")
        .await
}
// ANCHOR_END: domain_tree_await_request

/// Awaits at most two pages as an explicit prefix.
///
/// The returned [`Prefix`] carries its traversal summary, so a page bound is
/// never mistaken for exhaustion.
// ANCHOR: domain_tree_await_pagination
pub async fn fetch_two_pages(
    client: &CatalogApi,
) -> Result<Prefix<ItemSummary>, concord::ApiError> {
    client
        .regional("eu")
        .item_summaries()
        .collect_prefix(2)
        .await
}
// ANCHOR_END: domain_tree_await_pagination
