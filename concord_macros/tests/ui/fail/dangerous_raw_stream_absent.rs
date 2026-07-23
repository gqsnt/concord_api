use concord_macros::api;

api! {
    client RawStreamApi {
        base "http://raw-stream.example.test/";
    }

    GET Download() -> Stream<OctetStream>;
}

fn main() {
    let client = RawStreamApi::new().expect("client construction");
    let _ = client.download().raw_response();
}
