use concord_core::advanced::{Method, PreparedEndpoint};
use concord_macros::api;

api! {
    client RawPreparedApi {
        base "http://raw-prepared.example.test/";
    }

    GET Ordinary() -> Bytes;
}

fn main() {
    let client = RawPreparedApi::new().expect("client construction");
    let endpoint = PreparedEndpoint::builder("Prepared::bytes", Method::GET)
        .response_bytes()
        .build()
        .expect("prepared endpoint");
    let _ = client.request(endpoint).raw_response();
}
