// Explicit-endpoint value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client EndpointSetterApi {
        base "http://endpoint-setter.example.test";
    }

    GET read(label: Option<String>) -> Bytes;
}

fn main() {
    let _ = endpoint_setter_api::Read::new().label("selected".to_owned());
}
