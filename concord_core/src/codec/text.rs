use crate::codec::{
    BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use crate::codec::{ContentType, Decodes, Encodes, Format, FormatType};
use bytes::Bytes;
use std::marker::PhantomData;
use std::str::Utf8Error;

pub struct Text<T = String>(PhantomData<T>);

impl ContentType for Text {
    const CONTENT_TYPE: &'static str = "text/plain";
}

impl FormatType for Text {
    const FORMAT_TYPE: Format = Format::Text;
}

impl<T> Encodes<T> for Text
where
    T: AsRef<str>,
{
    type Error = std::convert::Infallible;
    fn encode(output: &T) -> Result<Bytes, Self::Error> {
        Ok(Bytes::copy_from_slice(output.as_ref().as_bytes()))
    }
}

impl Decodes<String> for Text {
    type Error = Utf8Error;
    fn decode(bytes: &Bytes) -> Result<String, Self::Error> {
        Ok(std::str::from_utf8(bytes)?.to_string())
    }
}

impl<T> BodyCodec for Text<T>
where
    T: AsRef<str> + Send + Sync + 'static,
{
    type Value = T;

    fn content_type() -> &'static str {
        <Text as ContentType>::CONTENT_TYPE
    }

    fn format() -> Format {
        <Text as FormatType>::FORMAT_TYPE
    }

    fn encode(value: &Self::Value, _ctx: EncodeContext) -> Result<EncodedBody, CodecError> {
        <Text as Encodes<T>>::encode(value)
            .map(|bytes| {
                EncodedBody::from_bytes(bytes)
                    .with_content_type(<Text as ContentType>::CONTENT_TYPE)
                    .text()
            })
            .map_err(|err| CodecError::with_source("text encode failed", err))
    }
}

impl ResponseCodec for Text<String> {
    type Value = String;

    fn accept() -> &'static str {
        <Text as ContentType>::CONTENT_TYPE
    }

    fn format() -> Format {
        <Text as FormatType>::FORMAT_TYPE
    }

    fn decode(bytes: &Bytes, _ctx: DecodeContext) -> Result<Self::Value, CodecError> {
        <Text as Decodes<String>>::decode(bytes)
            .map_err(|err| CodecError::with_source("text decode failed", err))
    }
}
