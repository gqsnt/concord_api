use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client RawRuntimeApi {
        base "http://raw-runtime.example.test/";
    }

    GET ordinary() -> Bytes;
}

fn main() {
    let client = RawRuntimeApi::new().expect("client construction");
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("runtime.bytes").unwrap(), Method::GET)
        .response_bytes()
        .build()
        .expect("runtime endpoint");
    let _ = client.request(endpoint).raw_response();
}
