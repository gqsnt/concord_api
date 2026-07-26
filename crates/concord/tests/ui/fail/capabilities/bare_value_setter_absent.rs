// Generated value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
    }

    GET read(verbose: Option<bool>) -> Bytes;
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.read().verbose(true);
}
