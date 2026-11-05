//! SH-A against a live shop (read-only GraphQL queries): `SHOPIFY_SHOP`, `SHOPIFY_ADMIN_TOKEN`,
//! optional `SHOPIFY_LOCATION_ID` (`gid://shopify/Location/..`). Without credentials it prints
//! `NOT_RUN` and exits 0. The page budget is small (3 pages per traversal) so a smoke run stays
//! within a bounded request count.

#[path = "common/live.rs"]
mod live;

use std::num::NonZeroUsize;

use concord_example_shopify_app::catalog_inventory::{ReconcileRequest, reconcile};
use concord_example_shopify_sdk::scalars::LocationId;
use concord_pagination::Budget;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(shop) = live::shop_from_env()? else {
        println!("NOT_RUN sh_a: SHOPIFY_SHOP and SHOPIFY_ADMIN_TOKEN are not set");
        return Ok(());
    };
    let mut request = ReconcileRequest::new(Budget::new(NonZeroUsize::new(3).unwrap()));
    request.nested_traversals = 10;
    if let Ok(id) = std::env::var("SHOPIFY_LOCATION_ID") {
        request.location = Some(LocationId::parse(&id)?);
    }
    let report = reconcile(&shop, &request).await;
    println!("{report:#?}");
    Ok(())
}
