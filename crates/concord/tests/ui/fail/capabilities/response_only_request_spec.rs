use concord::response::{Discard, ResponseBodyError, ResponseSpec, ResponseHead};
use concord::{Bytes, api};

pub struct ResponseOnly;

impl ResponseSpec for ResponseOnly {
    type Output = ();
    type Mode = Discard;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

api! {
    client InvalidResponseOnlyRequestApi {
        timeout off;
        base "http://invalid-response-only.example.test/";


    POST invalid(body body: Bytes using crate::ResponseOnly) -> Bytes using Bytes;
}}

fn main() {}
