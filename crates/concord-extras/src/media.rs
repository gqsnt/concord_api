//! Response `Content-Type` parsing shared by the formats of this crate.

use concord_core::{CodecError, ContentTypeField, ResponseHead};

/// The response's single `Content-Type`, parsed by `mime` (case-insensitive type, subtype and
/// parameter names), with its text. `Ok(None)`: no field. A repeated field (equal or not), a value
/// that is not visible ASCII, and one `mime` rejects (a comma-joined list among them) are errors.
pub(crate) fn single<'a>(
    head: &ResponseHead<'a>,
) -> Result<Option<(mime::Mime, &'a str)>, CodecError> {
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
    match text.parse::<mime::Mime>() {
        Ok(media) => Ok(Some((media, text))),
        Err(e) => {
            Err(CodecError::new("the response Content-Type is not a media type").with_source(e))
        }
    }
}

/// Whether a `Content-Type` value's `charset` parameter, if any, names UTF-8 (`us-ascii` is a
/// subset, so it is accepted too); charset names are case-insensitive. A repeated `charset` is
/// refused as ambiguous, whatever its values.
///
/// The parameters are read here rather than through `mime`, whose 0.3.17 release misreads a
/// quoted value followed by another parameter (`charset="utf-8"; x=y` reads as `"utf-`): only the
/// `charset` values are extracted, with the RFC 9110 parameter grammar (tokens and quoted strings
/// with quoted pairs). `mime` has already accepted the value.
pub(crate) fn check_charset(text: &str, foreign: &'static str) -> Result<(), CodecError> {
    let Some(charsets) = charsets(text) else {
        return Err(CodecError::new(
            "the response Content-Type is not a media type",
        ));
    };
    charset_verdict(&charsets, foreign)
}

/// [`check_charset`] on already extracted values.
pub(crate) fn charset_verdict(
    charsets: &[String],
    foreign: &'static str,
) -> Result<(), CodecError> {
    match charsets {
        [] => Ok(()),
        [one] if one.eq_ignore_ascii_case("utf-8") || one.eq_ignore_ascii_case("us-ascii") => {
            Ok(())
        }
        [_] => Err(CodecError::new(foreign)),
        _ => Err(CodecError::new(
            "the response Content-Type declares charset more than once",
        )),
    }
}

/// The values of every `charset` parameter of a media type, unquoted; `None` if the parameter
/// section does not follow `*( OWS ";" OWS [ token "=" ( token / quoted-string ) ] )`.
pub(crate) fn charsets(text: &str) -> Option<Vec<String>> {
    let tchar = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    let bytes = text.as_bytes();
    let mut i = bytes.iter().position(|&b| b == b';').unwrap_or(bytes.len());
    let mut found = Vec::new();
    let skip_ows = |i: &mut usize| {
        while *i < bytes.len() && matches!(bytes[*i], b' ' | b'\t') {
            *i += 1;
        }
    };
    loop {
        skip_ows(&mut i);
        if i == bytes.len() {
            return Some(found);
        }
        if bytes[i] != b';' {
            return None;
        }
        i += 1;
        skip_ows(&mut i);
        if i == bytes.len() || bytes[i] == b';' {
            continue; // an empty parameter
        }
        let start = i;
        while i < bytes.len() && tchar(bytes[i]) {
            i += 1;
        }
        if i == start || i == bytes.len() || bytes[i] != b'=' {
            return None;
        }
        let name = &text[start..i];
        i += 1;
        let value = if bytes.get(i) == Some(&b'"') {
            let mut value = String::new();
            i += 1;
            loop {
                match bytes.get(i) {
                    None => return None,
                    Some(b'"') => {
                        i += 1;
                        break;
                    }
                    Some(b'\\') => {
                        let escaped = *bytes.get(i + 1)?;
                        value.push(char::from(escaped));
                        i += 2;
                    }
                    Some(&b) => {
                        value.push(char::from(b));
                        i += 1;
                    }
                }
            }
            value
        } else {
            let start = i;
            while i < bytes.len() && tchar(bytes[i]) {
                i += 1;
            }
            if i == start {
                return None;
            }
            text[start..i].to_owned()
        };
        if name.eq_ignore_ascii_case("charset") {
            found.push(value);
        }
    }
}
