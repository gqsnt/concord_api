//! `Text`: UTF-8 text.

use concord_core::bytes::Bytes;
use concord_core::{BodyError, Decode, Encode, reqwest};

/// UTF-8 text: `text/plain; charset=utf-8` request bodies; responses must be valid UTF-8.
/// `request Text;`, `response Text;`.
pub struct Text(());

impl Encode<String> for Text {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode<String> for Text {
    fn decode(body: Bytes) -> Result<String, BodyError> {
        String::from_utf8(body.into()).map_err(|e| {
            BodyError::new(format!(
                "response body is not valid UTF-8 (valid up to byte {})",
                e.utf8_error().valid_up_to()
            ))
        })
    }
}
