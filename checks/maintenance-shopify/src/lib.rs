//! Staged maintenance exercise for the Shopify SDK slice (`examples/shopify-sdk`), see
//! `MAINTENANCE.md`.
//!
//! [`before`] is the frozen initial consumer: a downstream product-feed job written against the
//! SDK's public exports only. It is compiled unchanged against every later SDK revision.
//! [`after`] is the updated consumer: the same job using the SDK change (a product's storefront
//! link on request, product metafields in the bulk export), reusing the frozen consumer's types.

pub mod after;
pub mod before;
