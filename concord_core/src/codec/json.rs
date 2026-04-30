use crate::codec::*;
use bytes::Bytes;
use http::HeaderValue;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;

pub struct Json<T = ()>(PhantomData<T>);

impl ContentType for Json {
    const CONTENT_TYPE: &'static str = "application/json";
}

impl FormatType for Json {
    const FORMAT_TYPE: Format = Format::Text;
}

impl<T> Encodes<T> for Json
where
    T: Serialize,
{
    type Error = serde_json::Error;
    fn encode(output: &T) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(output).map(Bytes::from)
    }
}

impl<T> Decodes<T> for Json
where
    T: DeserializeOwned,
{
    type Error = serde_json::Error;
    fn decode_owned(bytes: Bytes) -> Result<T, Self::Error> {
        serde_json::from_slice(&bytes)
    }
}

impl<T> BodyCodec for Json<T>
where
    T: Serialize + Send + Sync + 'static,
{
    type Value = T;

    fn content_type() -> Option<HeaderValue> {
        Some(HeaderValue::from_static(
            <Json as ContentType>::CONTENT_TYPE,
        ))
    }

    fn format() -> Format {
        <Json as FormatType>::FORMAT_TYPE
    }

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        <Json as Encodes<T>>::encode(&value)
            .map(|bytes| EncodedBody::from_bytes(bytes).text())
            .map_err(|err| CodecError::with_source("json encode failed", err))
    }
}

impl<T> ResponseCodec for Json<T>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    type Value = T;

    fn accept() -> Option<HeaderValue> {
        Some(HeaderValue::from_static(
            <Json as ContentType>::CONTENT_TYPE,
        ))
    }

    fn format() -> Format {
        <Json as FormatType>::FORMAT_TYPE
    }

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        <Json as Decodes<T>>::decode_owned(bytes)
            .map_err(|err| CodecError::with_source("json decode failed", err))
    }
}
