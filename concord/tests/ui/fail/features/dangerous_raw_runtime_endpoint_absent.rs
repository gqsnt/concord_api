use concord::advanced::{Method, RuntimeEndpoint};
use concord::api;

api! {
    client RawRuntimeApi {
        base "http://raw-runtime.example.test/";
    }

    GET ordinary() -> Bytes;
}

fn main() {
    let client = RawRuntimeApi::new().expect("client construction");
    let endpoint = RuntimeEndpoint::builder("runtime.bytes".into(), Method::GET)
        .response_bytes()
        .build()
        .expect("runtime endpoint");
    let _ = client.request(endpoint).raw_response();
}
