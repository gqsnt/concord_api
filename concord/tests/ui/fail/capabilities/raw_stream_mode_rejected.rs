use concord::{OctetStream, Stream, api};

api! {
    client StreamApi {
        base "http://stream.example.test/";
    }

    GET download() -> Stream<OctetStream>;
}

fn main() {
    let client = StreamApi::new().unwrap();
    let _ = client.download().raw_response();
}
