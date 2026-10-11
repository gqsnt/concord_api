// HEAD-01 / §9.2: the Accept header is owned by the selected response
// representation; a runtime builder has no `accept` override either.
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

fn main() {
    let _ = RuntimeEndpoint::builder(
        EndpointId::new("runtime.accept-override").unwrap(),
        Method::GET,
    )
    .accept("application/json");
}
