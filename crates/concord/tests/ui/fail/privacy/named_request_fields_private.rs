// §4.3 / §7.2: a required uninitialized endpoint value is a positional
// argument (it cannot be skipped), and a generated named request exposes no
// fields: its inner state is private.
use concord::{Binary, Bytes, api};

api! {
    client SetterApi {
        timeout 30s;
        base "http://setter.example.test";

        GET item(query verbose: bool) -> Bytes via Binary;
    }
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.item();
    let request = client.item(true);
    let _ = request.__inner;
}
