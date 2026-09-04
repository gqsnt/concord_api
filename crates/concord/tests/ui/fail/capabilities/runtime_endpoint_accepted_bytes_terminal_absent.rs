use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client RuntimeAcceptedBytesApi {
        timeout 30s;
        base "https://example.test";

        GET ordinary() -> concord::Bytes via concord::Bytes;
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
    let request = endpoint.try_bind(&client).unwrap();
    let _ = request.accepted_bytes();
}
