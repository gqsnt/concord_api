use concord::{Bytes, api};

api! {
    client FirstApi {
        timeout off;
        base "http://first.example.test/";


    GET read() -> Bytes using Bytes;
}}

api! {
    client SecondApi {
        timeout off;
        base "http://second.example.test/";


    GET read() -> Bytes using Bytes;
}}

fn main() {
    let client = SecondApi::new().unwrap();
    let _ = client.request(FirstApi::unbound().read());
}
