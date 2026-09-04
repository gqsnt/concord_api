use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client RuntimePaginationApi {
        timeout 30s;
        base "https://example.test";


    GET ordinary() -> Bytes via Binary;
}}

fn main() {
    let client = RuntimePaginationApi::new().unwrap();
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("runtime.pages").unwrap(), Method::GET)
        .accepted_bytes()
        .build()
        .unwrap();
    let request = endpoint.try_bind(&client).unwrap();
    let _ = request.paginate(1_u32);
}
