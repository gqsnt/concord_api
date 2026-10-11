// VALUE-03: request value mutation is `set_verbose`/`replace_verbose`/...;
// there is no legacy bare-method alias.
use concord::{Binary, Bytes, api};

api! {
    client SetterApi {
        timeout 30s;
        base "http://setter.example.test";

        GET read(query verbose?: bool) -> Bytes via Binary;
    }
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.read().verbose(true);
}
