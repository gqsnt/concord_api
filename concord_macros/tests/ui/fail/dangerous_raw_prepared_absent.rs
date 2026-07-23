use concord_core::advanced::{Method, RuntimeEndpoint};
use concord_macros::api;

api! {
    client RawPreparedApi {
        base "http://raw-prepared.example.test/";
    }

    GET ordinary() -> Bytes;
}

fn main() {
    let client = RawPreparedApi::new().expect("client construction");
    let endpoint = RuntimeEndpoint::builder("runtime.bytes".into(), Method::GET)
        .response_bytes()
        .build()
        .expect("runtime endpoint");
    let _ = client.request(endpoint).raw_response();
}
