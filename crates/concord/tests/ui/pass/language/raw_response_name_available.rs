use concord::{Bytes, api};

api! {
    client NamingApi {
        timeout 30s;
        base "http://example.test";


    GET raw_response() -> Bytes via Binary {
        expose accepted_bytes;
    }
}}

fn main() {
    let client = NamingApi::new().expect("client");
    let _ = client.raw_response().accepted_bytes();
}
