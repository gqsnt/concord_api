// §7.3 / VALUE-03: a value control belongs only to the request wrapper of the
// endpoint that declares the value; `set_verbose` exists on `FirstRequest`
// but not on `SecondRequest`.
use concord::{Binary, Bytes, api};

api! {
    client SetterApi {
        timeout 30s;
        base "http://setter.example.test";

        GET first(query verbose?: bool) -> Bytes via Binary;
        GET second() -> Bytes via Binary;
    }
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.first().set_verbose(true);
    let _ = client.second().set_verbose(true);
}
