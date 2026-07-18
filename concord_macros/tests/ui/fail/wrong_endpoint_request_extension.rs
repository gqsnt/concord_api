use concord_macros::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
        GET First(verbose: Option<bool>) -> Bytes;
        GET Second() -> Bytes;
    }
}

fn main() {
    use setter_api::prelude::*;

    let client = SetterApi::new().unwrap();
    let _ = client.second().verbose(true);
}
