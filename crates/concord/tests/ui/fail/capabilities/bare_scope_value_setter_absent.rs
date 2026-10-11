// VALUE-03: scope-handle value mutation is `set_label`/`replace_label`/...;
// there is no legacy bare-method alias.
use concord::{Binary, Bytes, api};

api! {
    client ScopeSetterApi {
        timeout 30s;
        base "http://scope-setter.example.test";

        scope control_scope(query label?: String) {
            GET list() -> Bytes via Binary;
        }
    }
}

fn main() {
    let client = ScopeSetterApi::new().unwrap();
    let _ = client.control_scope().label("selected".to_owned());
}
