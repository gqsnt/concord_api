use concord::{Bytes, api};

api! {
    client FirstApi {
        timeout 30s;
        base "http://first.example.test/";


    GET read() -> Bytes via Binary;
}}

api! {
    client SecondApi {
        timeout 30s;
        base "http://second.example.test/";


    GET read() -> Bytes via Binary;
}}

fn main() {
    let client = SecondApi::new().unwrap();
    let _ = client.request(FirstApi::unbound().read());
}
