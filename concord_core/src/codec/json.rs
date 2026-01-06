use crate::codec::*;
use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub struct JsonEncoding;

impl ContentType for JsonEncoding {
    const CONTENT_TYPE: &'static str = "application/json";
}

impl FormatType for JsonEncoding {
    const FORMAT_TYPE: Format = Format::Text;
}

impl<T> Encodes<T> for JsonEncoding
where
    T: Serialize,
{
    type Error = serde_json::Error;
    fn encode(output: &T) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(output).map(Bytes::from)
    }
}

impl<T> Decodes<T> for JsonEncoding
where
    T: DeserializeOwned,
{
    type Error = serde_json::Error;
    fn decode(bytes: &Bytes) -> Result<T, Self::Error> {
        serde_json::from_slice(bytes)
    }
}
