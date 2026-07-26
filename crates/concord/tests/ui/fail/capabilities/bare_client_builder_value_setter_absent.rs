// Client-builder value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client BuilderSetterApi {
        vars {
            label: Option<String>;
        }
        base "http://builder-setter.example.test";
    }

    GET read() -> Bytes;
}

fn main() {
    let _ = BuilderSetterApi::builder().label("selected".to_owned());
}
