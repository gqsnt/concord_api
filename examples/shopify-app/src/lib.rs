//! The external caller of `concord-example-shopify-sdk`: the two required Shopify workflows as
//! library functions over a configured [`ShopContext`](concord_example_shopify_sdk::ShopContext).
//!
//! | Workflow | Function | Outcome type |
//! |---|---|---|
//! | SH-A catalog and inventory reconciliation | [`catalog_inventory::reconcile`] | [`catalog_inventory::ReconcileReport`] |
//! | SH-B bulk catalog export | [`bulk_export::export_catalog`] | [`bulk_export::ExportReport`] (records go to a caller's [`bulk_export::RecordSink`]) |
//!
//! It uses only the SDK's public exports, `concord`, `concord-pagination` and the standard
//! library. The binaries `sh_a` and `sh_b` run the same functions against a live shop when
//! credentials are set, and print `NOT_RUN` otherwise (see the README).

pub mod bulk_export;
pub mod catalog_inventory;
pub mod cost;
