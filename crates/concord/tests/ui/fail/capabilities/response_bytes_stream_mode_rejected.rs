use concord::api;
use concord::body::Stream;
use concord::codec::OctetStream;

api! {
    client StreamApi {
        base "http://stream.example.test/";
    }

    GET download() -> Stream<OctetStream>;
}

fn main() {
    let client = StreamApi::new().unwrap();
    let _ = client.download().response_bytes();
}
