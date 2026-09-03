// An inherent setter belongs only to its declared endpoint request wrapper.
use concord::api;

api! {
    client SetterApi {
        timeout off;
        base "http://setter.example.test";


    GET first(verbose: Option<bool>) -> Bytes using Bytes;
    GET second() -> Bytes using Bytes;
}}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.second().verbose(true);
}
