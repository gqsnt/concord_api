// Generated value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client SetterApi {
        timeout 30s;
        base "http://setter.example.test";


    GET read(verbose: Option<bool>) -> Bytes via Binary;
}}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.read().verbose(true);
}
