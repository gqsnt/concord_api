//! `Form`: URL-encoded forms via `serde_urlencoded`.

use concord_core::bytes::Bytes;
use concord_core::{CodecError, Decode, Encode, ResponseHead, reqwest};

use crate::media;

/// URL-encoded forms via `serde_urlencoded` (feature `form`).
///
/// - **Requests:** any `Serialize` struct, map or sequence of pairs with flat values (nested
///   structures and sequences are an encoding error), sent as
///   `application/x-www-form-urlencoded`.
/// - **Responses:** any `DeserializeOwned`, after a check of the response head made before the
///   body is read: exactly one well-formed `Content-Type` field,
///   `application/x-www-form-urlencoded`, with no `charset` other than UTF-8. Anything else is
///   an [`ErrorKind::Metadata`](concord_core::ErrorKind::Metadata) error; select
///   [`IgnoreContentType<Form>`](crate::IgnoreContentType) to skip that check deliberately.
/// - **`Accept`:** suggests `application/x-www-form-urlencoded`.
pub struct Form(());

impl<T: serde::Serialize> Encode<T> for Form {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: T) -> Result<reqwest::Body, CodecError> {
        serde_urlencoded::to_string(&value)
            .map(reqwest::Body::from)
            .map_err(|e| CodecError::new("the value cannot be encoded as a form").with_source(e))
    }
}

impl<T: serde::de::DeserializeOwned> Decode<T> for Form {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        let Some(found) = media::single(head)? else {
            return Err(CodecError::new("the response has no Content-Type"));
        };
        if found.essence.essence_str() != mime::APPLICATION_WWW_FORM_URLENCODED.essence_str() {
            return Err(CodecError::new(
                "the response Content-Type is not application/x-www-form-urlencoded",
            ));
        }
        media::check_charset(
            &found.charsets,
            "the form response declares a charset other than utf-8",
        )
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<T, CodecError> {
        serde_urlencoded::from_bytes(&body)
            .map_err(|e| CodecError::new("the body is not a valid form").with_source(e))
    }
}
