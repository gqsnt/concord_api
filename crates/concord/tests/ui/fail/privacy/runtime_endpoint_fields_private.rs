use concord::endpoint::{EndpointId, RuntimeEndpoint, RuntimeEndpointBuilder};
use concord::http::Method;
use concord::NoContent;

fn main() {
    let _ = RuntimeEndpointBuilder::<NoContent> {};
    let _ = RuntimeEndpoint::<NoContent> {};
    let _ = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.private").unwrap(),
        Method::GET,
    );
}
