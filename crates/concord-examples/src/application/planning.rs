//! Public planning and inert terminal selection over the canonical V3 Catalog.

// ANCHOR: catalog_planning
use std::num::NonZeroU32;

use concord::ApiError;

use crate::contracts::catalog_v3::{CatalogApi, Item, ItemSummary};

/// Inspects public request shapes and selects terminals without executing them.
///
/// # Errors
///
/// Returns a public head-planning error if a declared value cannot be encoded.
pub fn inspect_requests(client: &CatalogApi) -> Result<(), ApiError> {
    let regional = client.regional("eu-west".to_owned());
    let request = regional.item_summaries(Vec::new());
    let _plan = request.plan()?;
    let _complete: concord::pagination::CollectAll<ItemSummary> =
        request.collect_all(NonZeroU32::new(20).expect("positive"));
    let _accepted: concord::endpoint::AcceptedBytesRequest =
        regional.item("item-42".to_owned()).accepted_bytes();

    let endpoint = CatalogApi::unbound()
        .regional("eu-west".to_owned())
        .item("item-42".to_owned());
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _response: concord::endpoint::ResponseRequest<Item> = request.response();
    Ok(())
}
// ANCHOR_END: catalog_planning
