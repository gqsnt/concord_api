// Explicit-endpoint value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client EndpointSetterApi {
        timeout 30s;
        base "http://endpoint-setter.example.test";


    GET read(label: Option<String>) -> Bytes via Binary;
}}

fn main() {
    let _ = EndpointSetterApi::unbound()
        .read(None)
        .label("selected".to_owned());
}
