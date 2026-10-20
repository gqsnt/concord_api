use concord::response::{ResponseBodyError, ResponseHead, ResponseRepresentation, Streaming};
use concord::{Binary, ByteStream, Discard, api};

pub struct CustomStream;

impl ResponseRepresentation<String> for CustomStream {
    type Driver = Streaming;

    fn from_body(_body: ByteStream, _head: &ResponseHead<'_>) -> Result<String, ResponseBodyError> {
        Ok(String::new())
    }
}

pub type AliasedStream = CustomStream;

api! {
    client ByteStreamResponse {
        base "http://example.test/";
        GET download() -> ByteStream via Binary {
            revalidate etag;
        }
    }
}

api! {
    client DiscardResponse {
        base "http://example.test/";
        GET discard() -> () via Discard {
            revalidate etag;
        }
    }
}

api! {
    client CustomStreamResponse {
        base "http://example.test/";
        response AliasedStream;
        GET custom() -> String {
            revalidate etag;
        }
    }
}

fn main() {}
