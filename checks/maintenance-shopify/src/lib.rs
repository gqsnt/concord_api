//! Staged maintenance exercise for the Shopify SDK slice (`examples/shopify-sdk`), see
//! `MAINTENANCE.md`.
//!
//! [`before`] is the frozen initial consumer: a downstream product-feed job written against the
//! SDK's public exports only. It is compiled unchanged against every later SDK revision.

pub mod before;
