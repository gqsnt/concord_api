//! `Binary`: raw bytes.

use concord_core::bytes::Bytes;
use concord_core::{CodecError, Decode, Encode, ResponseHead, reqwest};

/// Raw bytes.
///
/// - **Requests:** `Bytes` or `Vec<u8>`, sent as `application/octet-stream` unless the statement
///   declares the media type (`request Binary "image/png";`).
/// - **Responses:** `Bytes` or `Vec<u8>`, unchanged, whatever the `Content-Type` (`image/png`,
///   `application/pdf`, none at all): nothing about the head is checked.
/// - **`Accept`:** no suggestion.
pub struct Binary(());

impl Encode<Bytes> for Binary {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Bytes) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Encode<Vec<u8>> for Binary {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Vec<u8>) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode<Bytes> for Binary {
    type Context = ();

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<Bytes, CodecError> {
        Ok(body)
    }
}

impl Decode<Vec<u8>> for Binary {
    type Context = ();

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<Vec<u8>, CodecError> {
        Ok(body.into())
    }
}
