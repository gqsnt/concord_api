// A format that does not support the declared payload type is reported at the statement's format:
// on the endpoint, on a scope and on the client, with the `Encode`/`Decode` requirement.
// Two formats of this crate (Concord knows none): `Lines` carries `String`, `Raw` carries bytes.
use concord::bytes::Bytes;
use concord::{CodecError, Decode, Encode, ResponseHead, reqwest};
pub struct Lines;
impl Encode<String> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain");
    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        Ok(value.into())
    }
}
impl Decode<String> for Lines {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<String, CodecError> {
        String::from_utf8(body.into()).map_err(|_| CodecError::new("not UTF-8"))
    }
}
pub struct Raw;
impl Encode<Bytes> for Raw {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(value: Bytes) -> Result<reqwest::Body, CodecError> {
        Ok(value.into())
    }
}
impl Decode<Bytes> for Raw {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Bytes, CodecError> {
        Ok(body)
    }
}
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item { response Lines; }
        POST create(body item: Item) { request Raw; }
    }
}
concord::api! {
    pub client Scalar {
        base "http://x.test";
        POST count(body n: u64) { request Lines; }
        GET total() -> u64 { response Raw; }
    }
}
concord::api! {
    pub client Scoped {
        base "http://x.test";
        scope s {
            request Lines;
            response Raw;
            POST create(body n: u64) -> Item;
        }
    }
}
fn main() {}
