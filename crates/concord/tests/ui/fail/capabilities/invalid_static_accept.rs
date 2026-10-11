// §9.5: a representation's static `ACCEPT` is validated as an HTTP Accept
// value once, before any client exists. A value with an injected line break is
// rejected when the generated endpoint is compiled; no representation code
// runs to discover it and no request can carry it.
use concord::response::{DiscardDriver, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord::api;

pub struct Injected;

impl ResponseRepresentation<()> for Injected {
    type Driver = DiscardDriver;
    const ACCEPT: Option<&'static str> = Some("text/plain\r\nx-injected: 1");

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

api! {
    client InvalidAcceptApi {
        timeout 30s;
        base "http://invalid-accept.example.test/";

        GET probe() -> () via crate::Injected;
    }
}

fn main() {}
