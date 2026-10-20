// §9.1: representation roles are selected by direction; a response-only
// representation cannot encode a request body.
use concord::response::{DiscardDriver, ResponseBodyError, ResponseRepresentation, ResponseHead};
use concord::{Binary, Bytes, api};

pub struct ResponseOnly;

impl ResponseRepresentation<()> for ResponseOnly {
    type Driver = DiscardDriver;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

api! {
    client InvalidResponseOnlyRequestApi {
        timeout 30s;
        base "http://invalid-response-only.example.test/";

        POST invalid(body body: Bytes via crate::ResponseOnly) -> Bytes via Binary;
    }
}

fn main() {}
