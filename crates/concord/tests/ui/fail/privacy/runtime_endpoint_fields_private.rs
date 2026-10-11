use concord::endpoint::{EndpointId, RuntimeEndpoint, RuntimeEndpointBuilder};
use concord::http::Method;

fn main() {
    let _ = RuntimeEndpointBuilder::<(), ()> {};
    let _ = RuntimeEndpoint::<(), ()> {};
    let _ = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.private").unwrap(),
        Method::GET,
    );
}
