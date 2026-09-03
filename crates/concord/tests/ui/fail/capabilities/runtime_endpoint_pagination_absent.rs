use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client RuntimePaginationApi {
        timeout off;
        base "https://example.test";


    GET ordinary() -> Bytes using Bytes;
}}

fn main() {
    let client = RuntimePaginationApi::new().unwrap();
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("runtime.pages").unwrap(), Method::GET)
        .accepted_bytes()
        .build()
        .unwrap();
    let _ = client.request(endpoint).paginate(1_u32);
}
