// VALUE-03: client-builder value mutation is `set_label`/`replace_label`/
// `set_label_if_some`/`omit_label`; there is no legacy bare-method alias.
use concord::{Binary, Bytes, api};

api! {
    client BuilderSetterApi {
        timeout 30s;
        values {
            label?: String;
        }
        base "http://builder-setter.example.test";
        query { "label" = label; }

        GET read() -> Bytes via Binary;
    }
}

fn main() {
    let _ = BuilderSetterApi::builder().label("selected".to_owned());
}
