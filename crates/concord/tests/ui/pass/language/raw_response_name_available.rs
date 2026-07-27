use concord::{Bytes, api};

api! {
    client NamingApi {
        base "https://example.test";
    }

    GET raw_response() -> Bytes;
}

fn main() {
    let client = NamingApi::new().expect("client");
    let _ = client.raw_response().response_bytes();
}
