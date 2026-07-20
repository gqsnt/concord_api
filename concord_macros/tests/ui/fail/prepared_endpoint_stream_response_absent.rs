use concord_core::advanced::{Method, OctetStream, PreparedEndpoint};
use concord_macros::api;

api! {
    client PreparedStreamApi {
        base "https://example.test";
        GET Ordinary() -> Bytes;
    }
}

fn main() {
    let client = PreparedStreamApi::new().unwrap();
    let endpoint = PreparedEndpoint::builder("Prepared::stream", Method::GET)
        .response_stream::<OctetStream>()
        .build()
        .unwrap();
    let _ = client.request(endpoint).response();
}
