use crate::codec::{ContentType, Decodes, Encodes, Format, FormatType};
use bytes::Bytes;
use std::str::Utf8Error;

pub struct TextEncoding;

impl ContentType for TextEncoding {
    const CONTENT_TYPE: &'static str = "text/plain";
}

impl FormatType for TextEncoding {
    const FORMAT_TYPE: Format = Format::Text;
}

impl<T> Encodes<T> for TextEncoding
where
    T: AsRef<str>,
{
    type Error = std::convert::Infallible;
    fn encode(output: &T) -> Result<Bytes, Self::Error> {
        Ok(Bytes::copy_from_slice(output.as_ref().as_bytes()))
    }
}

impl Decodes<String> for TextEncoding {
    type Error = Utf8Error;
    fn decode(bytes: &Bytes) -> Result<String, Self::Error> {
        Ok(std::str::from_utf8(bytes)?.to_string())
    }
}
