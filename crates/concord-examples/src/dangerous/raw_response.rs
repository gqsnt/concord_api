//! Explicit bounded raw-success acquisition.
//!
//! Required features: `dangerous-raw-response` and `json`.
//! Mode: construction and deterministic local execution.
//! Documentation owner: `docs/reference/dangerous.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;
use concord::dangerous::RawResponseRequest;

api! {
    client RawResponseApi {
        base "http://raw-response.example.test/";
    }

    GET record() -> Json<Vec<u32>>;
}

/// Constructs the dangerous terminal without executing it.
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let client = RawResponseApi::builder().build()?;
    let _: RawResponseRequest<_> = client.record().raw_response();
    Ok(())
}
