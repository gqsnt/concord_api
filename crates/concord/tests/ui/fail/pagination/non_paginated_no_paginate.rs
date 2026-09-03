use concord::{Bytes, api};

api! {
    client PlainApi {
        timeout off;
        base "http://plain.example.test/";


    GET read() -> Bytes using Bytes;
}}

fn main() {
    let client = PlainApi::new().unwrap();
    let _ = client.read().paginate(1_u32);
}
