use runtime::{Bytes, api};

api! {
    client RawResponseAbsentApi {
        base "http://raw-response-absent.example.test/";
    }

    GET read() -> Bytes;
}

fn main() {
    let client = RawResponseAbsentApi::new().expect("client construction");
    let _ = client.read().raw_response();
}
