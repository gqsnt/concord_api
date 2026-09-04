// An inherent setter belongs only to its declared endpoint request wrapper.
use concord::api;

api! {
    client SetterApi {
        timeout 30s;
        base "http://setter.example.test";


    GET first(verbose: Option<bool>) -> Bytes via Binary;
    GET second() -> Bytes via Binary;
}}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.second().verbose(true);
}
