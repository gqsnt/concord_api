use concord::endpoint::{EndpointId, RuntimeEndpoint, RuntimeEndpointBuilder};
use concord::http::Method;
use concord::Empty;

fn main() {
    let _ = RuntimeEndpointBuilder::<Empty> {};
    let _ = RuntimeEndpoint::<Empty> {};
    let _ = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.private").unwrap(),
        Method::GET,
    );
}
