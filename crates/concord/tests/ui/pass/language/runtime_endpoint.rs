#![deny(deprecated)]

use concord::api;
use concord::endpoint::{
    EndpointId, RuntimeEndpoint, RuntimeEndpointBuilder, TimeoutSelection,
};
use concord::http::Method;
use concord::response::NoContent;

api! {
    client RuntimeSurfaceApi {
        base "http://runtime-surface.example.test/";
    }

    GET generated() -> NoContent;
}

fn main() {
    let client = RuntimeSurfaceApi::new().expect("zero-input client");
    let from_borrowed = EndpointId::try_from("runtime.borrowed").expect("borrowed runtime ID");
    let from_owned =
        EndpointId::try_from(String::from("runtime.owned")).expect("owned runtime ID");
    assert_eq!(from_borrowed.as_str(), "runtime.borrowed");
    assert_eq!(from_owned.as_str(), "runtime.owned");
    let builder: RuntimeEndpointBuilder<NoContent> = RuntimeEndpoint::builder(
        EndpointId::new("runtime.surface").expect("bounded runtime ID"),
        Method::GET,
    );
    let endpoint = builder
        .path_segment("runtime")
        .inherit_timeout()
        .build()
        .expect("runtime endpoint");
    assert_eq!(
        endpoint.description().timeout(),
        TimeoutSelection::InheritClient
    );
    let _ = client.plan(&endpoint).expect("safe runtime plan");
    let _: concord::Request<_> = client.request(endpoint);
}
