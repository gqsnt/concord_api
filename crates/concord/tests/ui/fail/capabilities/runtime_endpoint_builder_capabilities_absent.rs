use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

fn main() {
    let builder = RuntimeEndpoint::builder(EndpointId::new("runtime.route").unwrap(), Method::GET);
    let _ = builder.path("/raw/fragment");

    let builder = RuntimeEndpoint::builder(EndpointId::new("runtime.page").unwrap(), Method::GET);
    let _ = builder.paginate(1_u32);
}
