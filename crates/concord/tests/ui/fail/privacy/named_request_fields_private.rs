use concord::api;

api! {
    client SetterApi {
        timeout off;
        base "http://setter.example.test";


    GET item(verbose: Option<bool>) -> Bytes using Bytes;
}}

fn main() {
    let client = SetterApi::new().unwrap();
    let request = client.item();
    let _ = request.__concord_inner;
}
