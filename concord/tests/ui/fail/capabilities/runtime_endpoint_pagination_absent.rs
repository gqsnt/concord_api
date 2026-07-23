use concord::advanced::{Method, RuntimeEndpoint};
use concord::api;

api! {
    client RuntimePaginationApi {
        base "https://example.test";
    }

    GET ordinary() -> Bytes;
}

fn main() {
    let client = RuntimePaginationApi::new().unwrap();
    let endpoint = RuntimeEndpoint::builder("runtime.pages".into(), Method::GET)
        .response_bytes()
        .build()
        .unwrap();
    let _ = client.request(endpoint).paginate(1_u32);
}
