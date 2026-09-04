use concord::api;

api! {
    client SetterApi {
        timeout 30s;
        base "http://setter.example.test";


    GET item(verbose: Option<bool>) -> Bytes via Binary;
}}

fn main() {
    let client = SetterApi::new().unwrap();
    let request = client.item();
    let _ = request.__concord_inner;
}
