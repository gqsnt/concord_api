//! `Form`: URL-encoded forms via `serde_urlencoded`.

use concord_core::bytes::Bytes;
use concord_core::{BodyError, Decode, Encode, reqwest};

/// URL-encoded forms via `serde_urlencoded` (feature `form`):
/// `application/x-www-form-urlencoded` request bodies from any `Serialize` struct, map or
/// sequence of pairs (flat values only: nested structures and sequences are an encoding error);
/// responses decode into any `DeserializeOwned`. `request Form;`, `response Form;`.
pub struct Form(());

impl<T: serde::Serialize> Encode<T> for Form {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: T) -> Result<reqwest::Body, BodyError> {
        serde_urlencoded::to_string(&value)
            .map(reqwest::Body::from)
            .map_err(|e| BodyError::new("form encode error").with_source(e))
    }
}

impl<T: serde::de::DeserializeOwned> Decode<T> for Form {
    fn decode(body: Bytes) -> Result<T, BodyError> {
        serde_urlencoded::from_bytes(&body)
            .map_err(|e| BodyError::new("form decode error").with_source(e))
    }
}
