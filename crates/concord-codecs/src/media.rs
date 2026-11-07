//! Response `Content-Type` parsing shared by the formats of this crate.

use concord_core::{CodecError, ContentTypeField, ResponseHead};

/// Charset handling for [`check_content_type`]. No transcoding is performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    /// Allow no charset, or exactly one naming UTF-8 or US-ASCII (case-insensitive).
    Utf8,
    /// Ignore charset declarations after validating the media type's parameter syntax.
    Ignore,
}

/// Require exactly one well-formed `Content-Type` matching an accepted media type.
///
/// Entries are case-insensitive `type/subtype` strings without parameters, or
/// `type/*+suffix` rules (for example `application/*+json`). Exact entries include
/// the entire subtype: `application/json` does not match `application/json+xml`.
/// Suffix rules match only the final structured suffix and the named top-level type.
/// Other wildcards are unsupported; response media ranges are always refused.
/// Invalid entries never match; an empty list accepts nothing.
///
/// Uses the standard formats' private parser and static diagnostic labels for missing,
/// repeated, malformed or non-ASCII fields and charset failures. A nonmatching type has
/// the static label `the response Content-Type is not an accepted media type`.
/// Call this from [`Decode::check_content_type`](concord_core::Decode::check_content_type)
/// (or its incremental counterpart) to reject the head before reading the body.
pub fn check_content_type(
    head: &ResponseHead<'_>,
    accepted: &[&str],
    charset: Charset,
) -> Result<(), CodecError> {
    let Some(found) = single(head)? else {
        return Err(CodecError::new("the response has no Content-Type"));
    };
    if found.essence.type_() == mime::STAR || found.essence.subtype() == mime::STAR {
        return Err(CodecError::new(
            "the response Content-Type is a media range, not a media type",
        ));
    }
    let matches = accepted.iter().any(|entry| {
        let Some((kind, sub)) = entry.split_once('/') else {
            return false;
        };
        if !is_token(kind) || kind.contains('*') || !is_token(sub) {
            return false;
        }
        if let Some(suffix) = sub.strip_prefix("*+") {
            !suffix.is_empty()
                && !suffix.contains(['*', '+'])
                && found.essence.type_().as_str().eq_ignore_ascii_case(kind)
                && found
                    .essence
                    .suffix()
                    .is_some_and(|s| s.as_str().eq_ignore_ascii_case(suffix))
        } else {
            !sub.contains('*') && found.essence.essence_str().eq_ignore_ascii_case(entry)
        }
    });
    if !matches {
        return Err(CodecError::new(
            "the response Content-Type is not an accepted media type",
        ));
    }
    match charset {
        Charset::Utf8 => check_charset(
            &found.charsets,
            "the response declares a charset other than utf-8",
        ),
        Charset::Ignore => Ok(()),
    }
}

/// A response `Content-Type`: its media type and the values of its `charset` parameters.
pub(crate) struct Media {
    /// Type and subtype (and a structured-syntax suffix), parsed by `mime`, without parameters.
    /// Read by `Json`, `Form`, the JSON Lines and the SSE readers; `Text` needs only the charsets.
    pub(crate) essence: mime::Mime,
    /// Every `charset` parameter's value, unquoted, in order.
    pub(crate) charsets: Vec<String>,
    /// Every parameter, name lowercased and value unquoted, in order (`charset` included). Read by
    /// the streaming formats (`header=` of `text/csv`).
    #[cfg_attr(not(feature = "csv"), allow(dead_code))]
    pub(crate) params: Vec<(String, String)>,
}

/// The response's single `Content-Type`. `Ok(None)`: no field. A repeated field (equal or not), a
/// value that is not visible ASCII and one that is not a media type (a comma-joined list among
/// them) are errors.
///
/// `mime` parses only the type and subtype (case-insensitive); the parameters are read with the
/// RFC 9110 grammar by [`charsets`], because `mime` 0.3.17 misreads a quoted value followed by
/// another parameter (`charset="utf-8"; x=y` reads as `"utf-`) and refuses forms the grammar
/// allows (whitespace before `;`, an empty parameter).
pub(crate) fn single(head: &ResponseHead<'_>) -> Result<Option<Media>, CodecError> {
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
    let not_media = || CodecError::new("the response Content-Type is not a media type");
    let end = text.find(';').unwrap_or(text.len());
    let essence = text[..end].trim_end_matches([' ', '\t']);
    // `type "/" subtype`, both non-empty tokens: `mime` alone accepts an empty subtype (`text/`).
    let tokens = essence
        .split_once('/')
        .is_some_and(|(kind, sub)| is_token(kind) && is_token(sub));
    if !tokens {
        return Err(not_media());
    }
    let essence = essence
        .parse::<mime::Mime>()
        .map_err(|e| not_media().with_source(e))?;
    let params = params(text).ok_or_else(not_media)?;
    let charsets = params
        .iter()
        .filter(|(name, _)| name == "charset")
        .map(|(_, value)| value.clone())
        .collect();
    Ok(Some(Media {
        essence,
        charsets,
        params,
    }))
}

/// An RFC 9110 `token`: one or more `tchar`.
pub(crate) fn is_token(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(tchar)
}

/// An RFC 9110 `tchar`.
fn tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Whether a `charset` value names UTF-8 (`us-ascii` is a subset); case-insensitive.
pub(crate) fn is_utf8(charset: &str) -> bool {
    charset.eq_ignore_ascii_case("utf-8") || charset.eq_ignore_ascii_case("us-ascii")
}

/// Whether a media type's `charset` parameters, if any, name UTF-8 (`us-ascii` is a subset, so it
/// is accepted too); charset names are case-insensitive. A repeated `charset` is refused as
/// ambiguous, whatever its values. Used by `Json`, `Form`, the JSON Lines and the SSE readers;
/// `Text` applies its own rule with [`is_utf8`].
pub(crate) fn check_charset(charsets: &[String], foreign: &'static str) -> Result<(), CodecError> {
    match charsets {
        [] => Ok(()),
        [one] if is_utf8(one) => Ok(()),
        [_] => Err(CodecError::new(foreign)),
        _ => Err(CodecError::new(
            "the response Content-Type declares charset more than once",
        )),
    }
}

/// Every parameter of a media type, name lowercased and value unquoted, in order; `None` if the
/// parameter section does not follow `*( OWS ";" OWS [ token "=" ( token / quoted-string ) ] )`.
fn params(text: &str) -> Option<Vec<(String, String)>> {
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
        found.push((name.to_ascii_lowercase(), value));
    }
}
