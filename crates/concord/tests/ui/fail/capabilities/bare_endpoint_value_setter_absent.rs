// VALUE-03: unbound-intent value mutation is `set_label`/`replace_label`/...;
// there is no legacy bare-method alias.
use concord::{Binary, Bytes, api};

api! {
    client EndpointSetterApi {
        timeout 30s;
        base "http://endpoint-setter.example.test";

        GET read(query label?: String) -> Bytes via Binary;
    }
}

fn main() {
    let _ = EndpointSetterApi::unbound()
        .read()
        .label("selected".to_owned());
}
