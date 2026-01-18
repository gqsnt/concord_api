use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use bytes::Bytes;

#[cfg(feature = "json")]
pub(crate) mod json;

pub(crate) mod text;

pub enum Format {
    Binary,
    Text,
}

pub trait FormatType {
    const FORMAT_TYPE: Format;

    fn into_encoded_string(bytes: Bytes) -> String {
        match Self::FORMAT_TYPE {
            Format::Binary => STANDARD_NO_PAD.encode(bytes),
            Format::Text => String::from_utf8_lossy(bytes.as_ref()).to_string(),
        }
    }
}

pub(crate) fn format_bytes_for_debug(format: Format, bytes: &[u8], max_chars: usize) -> String {
    if max_chars == 0 { return String::new(); }
    match format {
        Format::Text => {
            // Worst case UTF-8 expansion for lossy preview: cap by ~4 bytes per char.
            let max_bytes = max_chars.saturating_mul(4).max(1);
            let slice_len = bytes.len().min(max_bytes);
            let s0 = String::from_utf8_lossy(&bytes[..slice_len]).to_string();
            let mut s = truncate_for_debug(&s0, max_chars);
            if slice_len < bytes.len() && !s.ends_with('…') {
                s.push('…');
            }
            s
        }
        Format::Binary => {
            // base64 expands 3 bytes -> 4 chars. Inverse: chars -> bytes ≈ (chars*3)/4.
            let max_bytes = max_chars.saturating_mul(3).div_ceil(4).max(1);
            let slice_len = bytes.len().min(max_bytes);
            let s0 = STANDARD_NO_PAD.encode(&bytes[..slice_len]);
            let mut s = truncate_for_debug(&s0, max_chars);
            if slice_len < bytes.len() && !s.ends_with('…') {
                s.push('…');
            }
            s
        }
    }
}

pub(crate) fn format_debug_body<F: FormatType>(bytes: &Bytes, max_chars: usize) -> String {
    format_bytes_for_debug(F::FORMAT_TYPE, bytes.as_ref(), max_chars)
}

pub(crate) fn truncate_for_debug(s: &str, max_chars: usize) -> String {
    if max_chars == 0 { return String::new(); }
    let mut it = s.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        match it.next() {
            Some(c) => out.push(c),
            None => return out,
        }
    }
    if it.next().is_some() {
        out.push('…');
    }
    out
}

pub trait ContentType {
    /// "" => pas de Content-Type/Accept pertinent.
    const CONTENT_TYPE: &'static str;
    const IS_NO_CONTENT: bool = false;
}

pub trait Decodes<T>: ContentType + FormatType {
    type Error: std::error::Error + Send + Sync + 'static;
    fn decode(bytes: &Bytes) -> Result<T, Self::Error>;
}

pub trait Encodes<T>: ContentType + FormatType {
    type Error: std::error::Error + Send + Sync + 'static;
    fn encode(output: &T) -> Result<Bytes, Self::Error>;
}

pub struct NoContent;

impl ContentType for NoContent {
    const CONTENT_TYPE: &'static str = "";
    const IS_NO_CONTENT: bool = true;
}

impl FormatType for NoContent {
    const FORMAT_TYPE: Format = Format::Text;
}

impl Encodes<()> for NoContent {
    type Error = std::convert::Infallible;
    fn encode(_output: &()) -> Result<Bytes, Self::Error> {
        Ok(Bytes::new())
    }
}

impl Decodes<()> for NoContent {
    type Error = std::convert::Infallible;
    fn decode(_bytes: &Bytes) -> Result<(), Self::Error> {
        Ok(())
    }
}
