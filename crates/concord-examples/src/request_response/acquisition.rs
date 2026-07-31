//! Direct decoded, metadata, explicit-byte, and accepted-status response paths.
//!
//! Required features: none. Mode: construction and deterministic local
//! execution. Documentation owners: `docs/responses.md` and `docs/runtime.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::http::StatusCode;
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead,
};
use concord::{ApiError, ApiResponse, Bytes, ClientBuildError, Text, api};

// ANCHOR: optional_response_spec
/// A buffered response that maps an accepted `404` to `None`.
pub struct OptionalText;

impl ResponseBodySpec for OptionalText {
    type Output = Option<String>;
    type Mode = Buffered;

    fn accept() -> Option<&'static str> {
        Some("text/plain")
    }

    fn accepts_status(status: StatusCode) -> bool {
        status.is_success() || status == StatusCode::NOT_FOUND
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Self::Output, ResponseBodyError> {
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
        base "http://responses.example.test/";
    }

    GET message() -> Text;
    GET optional_message() -> crate::request_response::acquisition::OptionalText;
}
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

// ANCHOR: response_bytes
/// Preserves declared acceptance and `Accept` while returning bounded bytes.
pub async fn as_bytes(client: &ResponseApi) -> Result<ApiResponse<Bytes>, ApiError> {
    client.message().response_bytes().await
}
// ANCHOR_END: response_bytes

// ANCHOR: accepted_not_found
/// Treats a declared `404` as an accepted optional result.
pub async fn optional(client: &ResponseApi) -> Result<Option<String>, ApiError> {
    client.optional_message().await
}
// ANCHOR_END: accepted_not_found
