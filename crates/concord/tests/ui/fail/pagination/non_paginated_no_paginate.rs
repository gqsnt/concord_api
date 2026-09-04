use concord::{Bytes, api};

api! {
    client PlainApi {
        timeout 30s;
        base "http://plain.example.test/";


    GET read() -> Bytes via Binary;
}}

fn main() {
    let client = PlainApi::new().unwrap();
    let _ = client.read().paginate(1_u32);
}
