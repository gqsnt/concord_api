use concord::advanced::{
    Method, NoContent, RuntimeEndpoint, RuntimeEndpointBuilder,
};

fn main() {
    let _ = RuntimeEndpointBuilder::<NoContent> {};
    let _ = RuntimeEndpoint::<NoContent> {};
    let _ = RuntimeEndpoint::builder("runtime.private".into(), Method::GET);
}
