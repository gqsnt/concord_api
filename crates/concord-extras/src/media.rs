//! Response `Content-Type` parsing shared by the formats of this crate.

use concord_core::{CodecError, ContentTypeField, ResponseHead};

/// The response's single `Content-Type`, parsed by `mime` (case-insensitive type, subtype and
/// parameter names). `Ok(None)`: no field. A repeated field (equal or not), a value that is not
/// visible ASCII, and one `mime` rejects (a comma-joined list among them) are errors.
pub(crate) fn single(head: &ResponseHead<'_>) -> Result<Option<mime::Mime>, CodecError> {
    let value = match head.content_type() {
        ContentTypeField::Missing => return Ok(None),
        ContentTypeField::Repeated => {
            return Err(CodecError::new(
                "the response has more than one Content-Type field",
            ));
        }
        ContentTypeField::One(value) => value,
    };
    let text = value
        .to_str()
        .map_err(|_| CodecError::new("the response Content-Type is not visible ASCII"))?;
    text.parse::<mime::Mime>().map(Some).map_err(|e| {
        CodecError::new("the response Content-Type is not a media type").with_source(e)
    })
}

/// Whether a parsed media type's `charset` parameter, if any, names UTF-8 (`us-ascii` is a
/// subset, so it is accepted too). Charset names are case-insensitive.
pub(crate) fn utf8_charset(media: &mime::Mime) -> bool {
    match media.get_param(mime::CHARSET) {
        None => true,
        Some(charset) => {
            charset == mime::UTF_8 || charset.as_str().eq_ignore_ascii_case("us-ascii")
        }
    }
}
