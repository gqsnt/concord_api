use concord::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord::{Binary, Bytes, Empty, Text, api};

pub struct CustomBuffered;

impl ResponseRepresentation<Vec<u8>> for CustomBuffered {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Vec<u8>, ResponseBodyError> {
        Ok(body.to_vec())
    }
}

pub type AliasedBuffered = CustomBuffered;

api! {
    client Revalidating {
        base "http://example.test/";
        response Text;

        GET text() -> String {
            revalidate etag;
        }

        GET bytes() -> Bytes via Binary {
            expose accepted_bytes;
            revalidate etag;
        }

        HEAD empty() -> () via Empty {
            revalidate etag;
        }

        POST explicitly_safe() -> String {
            effect safe;
            revalidate etag;
        }

        GET optional() -> Option<String> {
            absent on 404;
            revalidate etag;
        }

        scope custom() {
            response AliasedBuffered;
            GET value() -> Vec<u8> {
                revalidate etag;
            }
        }
    }
}

fn main() {
    let client = Revalidating::new().expect("client");
    let _ = client.text().plan().expect("plan");
    let _ = client.bytes().accepted_bytes();
    let _ = client.empty();
    let _ = client.explicitly_safe();
    let _ = client.optional();
    let _ = client.custom().value();
}
