use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

fn main() {
    let _ = RuntimeEndpoint::builder(
        EndpointId::new("runtime.accept-override").unwrap(),
        Method::GET,
    )
    .accept("application/json");
}
