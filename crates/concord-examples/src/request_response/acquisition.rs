//! Direct decoded, metadata, explicit-byte, and accepted-status response paths.
//!
//! Required features: none. Mode: construction and deterministic local
//! execution. Documentation owners: `docs/responses.md` and `docs/runtime.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::http::StatusCode;
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseHead, ResponseRepresentation,
};
use concord::{ApiError, ApiResponse, Bytes, ClientBuildError, Text, api};

// ANCHOR: optional_response_spec
/// A buffered response that maps an accepted `404` to `None`.
pub struct OptionalText;

impl ResponseRepresentation<Option<String>> for OptionalText {
    type Driver = Buffered;

    fn accept() -> Option<&'static str> {
        Some("text/plain")
    }

    fn from_body(
        body: Bytes,
        head: &ResponseHead<'_>,
    ) -> Result<Option<String>, ResponseBodyError> {
        if head.status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        String::from_utf8(body.to_vec())
            .map(Some)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}
// ANCHOR_END: optional_response_spec

// ANCHOR: response_acquisition_contract
api! {
    client ResponseApi {
        timeout 30s;
        base "http://responses.example.test/";


    GET message() -> String via Text {
        expose accepted_bytes;
    }
    GET optional_message() -> Option<String> via crate::request_response::acquisition::OptionalText {
        status [200..=299, 404];
    }
}}
// ANCHOR_END: response_acquisition_contract

/// Builds the no-input client directly.
pub fn construct() -> Result<ResponseApi, ClientBuildError> {
    ResponseApi::new()
}

// ANCHOR: decoded_response
/// Returns only the decoded application value.
pub async fn decoded(client: &ResponseApi) -> Result<String, ApiError> {
    client.message().await
}
// ANCHOR_END: decoded_response

// ANCHOR: metadata_response
/// Returns the decoded value together with accepted response metadata.
pub async fn with_metadata(client: &ResponseApi) -> Result<ApiResponse<String>, ApiError> {
    client.message().response().await
}
// ANCHOR_END: metadata_response

// ANCHOR: accepted_bytes
/// Preserves declared acceptance and `Accept` while returning bounded bytes.
pub async fn as_bytes(client: &ResponseApi) -> Result<ApiResponse<Bytes>, ApiError> {
    client.message().accepted_bytes().await
}
// ANCHOR_END: accepted_bytes

// ANCHOR: accepted_not_found
/// Treats a declared `404` as an accepted optional result.
pub async fn optional(client: &ResponseApi) -> Result<Option<String>, ApiError> {
    client.optional_message().await
}
// ANCHOR_END: accepted_not_found
