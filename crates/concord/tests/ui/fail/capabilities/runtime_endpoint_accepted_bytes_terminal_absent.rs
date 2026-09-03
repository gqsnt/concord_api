use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client RuntimeAcceptedBytesApi {
        timeout off;
        base "https://example.test";

        GET ordinary() -> concord::Bytes using concord::Bytes;
    }
}

fn main() {
    let client = RuntimeAcceptedBytesApi::new().unwrap();
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::new("runtime.accepted-bytes").unwrap(),
        Method::GET,
    )
    .accepted_bytes()
    .build()
    .unwrap();
    let _ = client.request(endpoint).accepted_bytes();
}
