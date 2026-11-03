//! `Binary`: raw bytes.

use concord_core::bytes::Bytes;
use concord_core::{BodyError, Decode, Encode, reqwest};

/// Raw bytes: `application/octet-stream` request bodies; responses are returned unchanged.
/// `request Binary;`, `request Binary "image/png";`, `response Binary;`.
pub struct Binary(());

impl Encode<Bytes> for Binary {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Bytes) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Encode<Vec<u8>> for Binary {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Vec<u8>) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode<Bytes> for Binary {
    fn decode(body: Bytes) -> Result<Bytes, BodyError> {
        Ok(body)
    }
}

impl Decode<Vec<u8>> for Binary {
    fn decode(body: Bytes) -> Result<Vec<u8>, BodyError> {
        Ok(body.into())
    }
}
