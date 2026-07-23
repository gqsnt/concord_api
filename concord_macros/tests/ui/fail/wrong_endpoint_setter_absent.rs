// An inherent setter belongs only to its declared endpoint request wrapper.
use concord_macros::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
    }

    GET First(verbose: Option<bool>) -> Bytes;
    GET Second() -> Bytes;
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.second().verbose(true);
}
