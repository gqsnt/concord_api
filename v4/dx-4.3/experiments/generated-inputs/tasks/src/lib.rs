//! Experiment G1's matched caller tasks. `baseline` uses only the V4.2 generated interface
//! (setters, navigation, `prepare`/`send`); `candidate` uses the experiment's generated input
//! records. Both solve the same three tasks over `concord-example-sdk`'s Catalog:
//!
//! - **T1 (DX01)** — a saved search stored by the application, run for two tenants, branched
//!   (one call without the inherited tags, one with its own), then edited, while requests
//!   built from earlier branches and the ordinary sibling keep their values.
//! - **T2 (DX02)** — clear the inherited tags and region for one call, check what the call will
//!   send before sending it, and keep the ordinary behaviour afterwards.
//! - **T3** — requests built elsewhere (handed over as values) are read and re-run for another
//!   tenant.
//!
//! The tests in `tests/tasks.rs` run both against the same fixture and require identical wire
//! requests.
#![allow(missing_docs)]

pub mod baseline;
#[cfg(feature = "candidate")]
pub mod candidate;

use concord_example_sdk::CatalogTenantListingList;

/// Requests built by code the task does not own: a "lamp search" for one tenant.
pub fn lamp_searches(
    listing: &concord_example_sdk::CatalogTenantListing,
) -> Vec<CatalogTenantListingList> {
    vec![
        listing.list().set_q("lamp").set_limit(5),
        listing.list().set_q("lamp").set_tags(Vec::new()),
    ]
}
