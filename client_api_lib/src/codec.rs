use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::{DecodeError, Engine};
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

    fn from_encoded_string(data: &str) -> Result<Bytes, DecodeError> {
        match Self::FORMAT_TYPE {
            Format::Binary => STANDARD_NO_PAD.decode(data).map(|data| data.into()),
            Format::Text => Ok(Bytes::copy_from_slice(data.as_bytes())),
        }
    }
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

pub struct NoContentEncoding;

impl ContentType for NoContentEncoding {
    const CONTENT_TYPE: &'static str = "";
    const IS_NO_CONTENT: bool = true;
}

impl FormatType for NoContentEncoding {
    const FORMAT_TYPE: Format = Format::Text;
}

impl Encodes<()> for NoContentEncoding {
    type Error = std::convert::Infallible;
    fn encode(_output: &()) -> Result<Bytes, Self::Error> {
        Ok(Bytes::new())
    }
}

impl Decodes<()> for NoContentEncoding {
    type Error = std::convert::Infallible;
    fn decode(_bytes: &Bytes) -> Result<(), Self::Error> {
        Ok(())
    }
}
