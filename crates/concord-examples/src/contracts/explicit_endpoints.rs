//! Shows that a generated endpoint value binds to the same named request type.
//!
//! Required features: none. Mode: construction-only.
//! Documentation owner: `docs/generated_client.md`.
//! Proving test: `tests/construction.rs`.

use super::full_dsl::{ReferenceApi, reference_api};

/// Constructs and binds a reusable explicit scoped endpoint.
///
/// The unbound intent captures the scope and endpoint snapshots; the selected
/// client supplies its root values and services at binding without evaluating
/// retained defaults again.
// ANCHOR: explicit_endpoint
pub fn construct(client: &ReferenceApi) -> reference_api::regions::FetchRequest {
    let endpoint = ReferenceApi::unbound()
        .regions("eu-west")
        .set_language("fr")
        .fetch("42", vec!["stable".to_owned()]);
    client.request(endpoint)
}
// ANCHOR_END: explicit_endpoint
