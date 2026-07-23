// An inherent setter belongs only to its declared endpoint request wrapper.
use concord::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
    }

    GET first(verbose: Option<bool>) -> Bytes;
    GET second() -> Bytes;
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.second().verbose(true);
}
