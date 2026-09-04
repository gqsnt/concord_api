//! Shows that a generated endpoint value binds to the same named request type.
//!
//! Required features: none. Mode: construction-only.
//! Documentation owner: `docs/generated_client.md`.
//! Proving test: `tests/construction.rs`.

use super::full_dsl::{ReferenceApi, reference_api};

/// Constructs and binds a reusable explicit scoped endpoint.
// ANCHOR: explicit_endpoint
pub fn construct(client: &ReferenceApi) -> reference_api::regions::FetchRequest {
    let endpoint = ReferenceApi::unbound()
        .regions("eu-west".to_owned())
        .set_language(Some("fr".to_owned()))
        .fetch("42".to_owned(), vec!["stable".to_owned()]);
    client.request(endpoint)
}
// ANCHOR_END: explicit_endpoint
