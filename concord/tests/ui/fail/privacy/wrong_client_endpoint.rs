use concord::{Bytes, api};

api! {
    client FirstApi {
        base "http://first.example.test/";
    }

    GET read() -> Bytes;
}

api! {
    client SecondApi {
        base "http://second.example.test/";
    }

    GET read() -> Bytes;
}

fn main() {
    let client = SecondApi::new().unwrap();
    let _ = client.request(first_api::Read::new());
}
