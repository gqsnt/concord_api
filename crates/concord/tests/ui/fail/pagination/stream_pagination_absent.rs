// CALL-03: a streaming endpoint has no page terminals; ordinary `.await`
// acquires one stream and never paginates.
use concord::{Binary, ByteStream, api};

api! {
    client StreamingApi {
        base "http://streaming.example.test";

        scope transfers() {
            path "transfers";
            GET download() -> ByteStream via Binary;
        }
    }
}

fn main() {
    let client = StreamingApi::new().unwrap();
    let _ = client.transfers().download().pages(1);
    let _ = client.transfers().download().collect_prefix(1);
}
