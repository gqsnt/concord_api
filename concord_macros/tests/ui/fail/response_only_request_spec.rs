use concord_core::advanced::{Discard, ResponseBodyError, ResponseBodySpec, ResponseHead};
use concord_macros::api;

pub struct ResponseOnly;

impl ResponseBodySpec for ResponseOnly {
    type Output = ();
    type Mode = Discard;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

api! {
    client InvalidResponseOnlyRequestApi {
        base "http://invalid-response-only.example.test/";
    }

    POST invalid(#[body] body: crate::ResponseOnly) -> Bytes;
}

fn main() {}
