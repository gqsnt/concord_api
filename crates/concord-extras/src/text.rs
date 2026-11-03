//! `Text`: UTF-8 text.

use concord_core::bytes::Bytes;
use concord_core::{CodecError, Decode, Encode, ResponseHead, reqwest};

use crate::media;

/// UTF-8 text.
///
/// - **Requests:** `String`, sent as `text/plain; charset=utf-8`.
/// - **Responses:** `String`. `Text` names the decoder, not a media type: any `Content-Type` is
///   accepted (`text/plain`, `text/csv`, `application/xml`), and so is a missing or malformed
///   one. The one refusal, made before the body is read, is a single well-formed `Content-Type`
///   whose `charset` parameter names something other than UTF-8 (`us-ascii` is accepted): the
///   bytes are not transcoded. Invalid UTF-8 is a decode error with the byte offset.
/// - **`Accept`:** no suggestion.
pub struct Text(());

impl Encode<String> for Text {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode<String> for Text {
    type Context = ();

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        // A missing, repeated or malformed `Content-Type` is accepted: `Text` names a decoder,
        // not a media type. A declared charset that is not UTF-8, or a repeated one, is refused.
        match media::single(head).map(|m| m.and_then(|(_, text)| media::charsets(text))) {
            Ok(Some(charsets)) => media::charset_verdict(
                &charsets,
                "the text response declares a charset other than utf-8",
            ),
            _ => Ok(()),
        }
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<String, CodecError> {
        String::from_utf8(body.into()).map_err(|e| {
            CodecError::new("the body is not valid UTF-8")
                .at("byte", e.utf8_error().valid_up_to() as u64)
        })
    }
}
