// An inherited format that cannot carry an endpoint's payload is reported at that endpoint's
// payload type (not at the distant default), with the trait context.
// Two formats of this crate (Concord knows none): `Lines` carries `String`, `Raw` carries bytes.
use concord::bytes::Bytes;
use concord::{BodyError, Decode, Encode, reqwest};
pub struct Lines;
impl Encode<String> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain");
    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Ok(value.into())
    }
}
impl Decode<String> for Lines {
    fn decode(body: Bytes) -> Result<String, BodyError> {
        String::from_utf8(body.into()).map_err(|_| BodyError::new("not UTF-8"))
    }
}
pub struct Raw;
impl Encode<Bytes> for Raw {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(value: Bytes) -> Result<reqwest::Body, BodyError> {
        Ok(value.into())
    }
}
impl Decode<Bytes> for Raw {
    fn decode(body: Bytes) -> Result<Bytes, BodyError> {
        Ok(body)
    }
}
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        request Raw;
        response Lines;
        GET item() -> Item;
        POST create(body item: Item);
    }
}
fn main() {}
