use concord::records::NdJson;
use concord::{ItemStream, api};

api! {
    client RecordStreamResponse {
        base "http://example.test/";
        GET records() -> ItemStream<String> via NdJson {
            revalidate etag;
        }
    }
}

fn main() {}
