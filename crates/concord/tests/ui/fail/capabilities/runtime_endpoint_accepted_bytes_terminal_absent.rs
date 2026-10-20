// CALL-03 / §10.4: selecting a complete-bytes response on a runtime builder
// (`accepted_bytes()` = `response::<Binary, Bytes>()`) chooses an output
// representation only; it does not expose the accepted-bytes terminal. A bound
// runtime request has no `accepted_bytes()` terminal without an explicit
// exposure.
use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client RuntimeAcceptedBytesApi {
        timeout 30s;
        base "https://example.test";

        GET ordinary() -> concord::Bytes via concord::Binary;
    }
}

fn main() {
    // §10.4: the accepted-bytes terminal requires a buffered or discard
    // driver; a streaming runtime response cannot select it.
    let client = RuntimeAcceptedBytesApi::new().unwrap();
    let endpoint = RuntimeEndpoint::builder(
        EndpointId::new("runtime.accepted-bytes").unwrap(),
        Method::GET,
    )
    .response::<concord::Binary, concord::ByteStream>()
    .expose_accepted_bytes()
    .build()
    .unwrap();
    let request = client.bind(endpoint).unwrap();
    let _ = request.accepted_bytes();
}
