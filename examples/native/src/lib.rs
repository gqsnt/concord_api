//! Concord V4 native-integration recipes, written as ordinary downstream application code.
//!
//! Every recipe uses the single [`api::Inventory`] declaration and the public Concord surface
//! only (`Prepared::{send, send_raw, into_parts}`, `Decoder::finish`, `Error::into_response`,
//! `Encode`/`Decode`, generated setters). Policies such as retries, pagination, admission and
//! credential changes live here, in the application, with their assumptions stated. See
//! `README.md` for the recipe ↔ task map and `tests/` for the loopback evidence.

pub mod api;
pub mod bodies;
pub mod errors;
pub mod native_edit;
pub mod policies;
pub mod repr;
pub mod streaming;
pub mod upload;
