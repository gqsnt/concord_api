// Client-builder value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client BuilderSetterApi {
        timeout 30s;
        values {
            label?: String;
        }
        base "http://builder-setter.example.test";


    GET read() -> Bytes via Binary;
}}

fn main() {
    let _ = BuilderSetterApi::builder().label("selected".to_owned());
}
