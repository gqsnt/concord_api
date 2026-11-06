//! `Encoded`: a body that is already encoded, sent with the media type bound to it.

use concord_core::{CodecError, Encode, EncodedBody, reqwest};

/// The format of a body that arrives already encoded, with its runtime media type: an
/// [`EncodedBody`], or (feature `stream`) a `stream::Upload` from a runtime
/// configured writer (a `CsvWriter` whose dialect was chosen per job).
///
/// It lets a declared body carry a body and its media type together, so the declaration states
/// the body it sends:
///
/// ```ignore
/// POST upload(id: String, body export: Upload) {
///     path "jobs/{id}/export";
///     request Encoded;
/// }
/// ```
///
/// The media type travels with the bytes ([`EncodedBody`] documents how it meets declared media
/// types and `content-type` headers: a contradiction is a `Build` error before anything is sent).
/// Preparing the request polls a lazy body zero times; the body is consumed once, never replayed.
/// [`encode`](Encode::encode) alone has nowhere to put the media type, so it refuses a body that
/// has one: Concord itself calls [`encode_body`](Encode::encode_body).
pub struct Encoded(());

impl Encode<EncodedBody> for Encoded {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(value: EncodedBody) -> Result<reqwest::Body, CodecError> {
        match value.into_parts() {
            (body, None) => Ok(body),
            (_, Some(_)) => Err(CodecError::new(
                "this body has a runtime media type; it is encoded with `encode_body`",
            )),
        }
    }

    fn encode_body(value: EncodedBody) -> Result<EncodedBody, CodecError> {
        Ok(value)
    }
}

#[cfg(feature = "stream")]
impl Encode<crate::stream::Upload> for Encoded {
    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_: crate::stream::Upload) -> Result<reqwest::Body, CodecError> {
        Err(CodecError::new(
            "an upload has a runtime media type; it is encoded with `encode_body`",
        ))
    }

    fn encode_body(value: crate::stream::Upload) -> Result<EncodedBody, CodecError> {
        Ok(value.into())
    }
}
