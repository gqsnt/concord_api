// Generated value mutation has no legacy bare-method alias.
use concord::api;

api! {
    client SetterApi {
        timeout off;
        base "http://setter.example.test";


    GET read(verbose: Option<bool>) -> Bytes using Bytes;
}}

fn main() {
    let client = SetterApi::new().unwrap();
    let _ = client.read().verbose(true);
}
