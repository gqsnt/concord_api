// A format's `Accept` suggestion is a compile-time-checked static header value, like its
// `Encode::CONTENT_TYPE`: an invalid one (a line break) fails where an endpoint uses the format.
use concord::bytes::Bytes;
use concord::{CodecError, Decode, ResponseHead};
pub struct Odd;
impl Decode<String> for Odd {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("text/plain\nx-injected: 1");
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<String, CodecError> {
        String::from_utf8(body.into()).map_err(|_| CodecError::new("not UTF-8"))
    }
}
concord::api! {
    pub client Api {
        base "http://x.test";
        GET odd() -> String { response Odd; }
    }
}
fn main() {}
