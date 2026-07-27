use concord::api;
use concord::body::Stream;

api! {
    client StreamApi {
        base "http://stream.example.test/";
    }

    GET download() -> Stream;
}

fn main() {
    let client = StreamApi::new().unwrap();
    let _ = client.download().response_bytes();
}
