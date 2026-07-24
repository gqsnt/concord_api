use concord::advanced::{
    EndpointId, Method, NoContent, RuntimeEndpoint, RuntimeEndpointBuilder,
};

fn main() {
    let _ = RuntimeEndpointBuilder::<NoContent> {};
    let _ = RuntimeEndpoint::<NoContent> {};
    let _ = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.private").unwrap(),
        Method::GET,
    );
}
