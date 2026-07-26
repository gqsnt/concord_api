//! Shows that a generated endpoint value binds to the same named request type.
//!
//! Required features: none. Mode: construction-only.
//! Documentation owner: `docs/guide/02-understand-generated-rust.md`.
//! Proving test: `tests/construction.rs`.

use super::full_dsl::{ReferenceApi, reference_api};

/// Constructs and binds a reusable explicit scoped endpoint.
pub fn construct(client: &ReferenceApi) -> reference_api::regions::FetchRequest {
    let endpoint = reference_api::regions::Fetch::new(
        "eu-west".to_owned(),
        "42".to_owned(),
        vec!["stable".to_owned()],
    )
    .set_language("fr".to_owned());
    client.request(endpoint)
}
