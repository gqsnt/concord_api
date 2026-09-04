use concord::api;
use concord::body::RequestStream;
use concord::{Binary, Bytes};

api! {
    client StreamApi {
        timeout 30s;
        base "http://stream.example.test/";


    POST upload(body body: ::concord::body::RequestStream via Binary) -> Bytes via Binary;
    GET download() -> ::concord::response::ByteStream via Binary;
    GET exact_binary() -> ::concord::response::ByteStream via Binary;
}}

fn main() {
    let client = StreamApi::new().unwrap();
    let _ = client.upload(RequestStream::from_bytes(Bytes::from_static(b"body")));
    let _ = client.download();
    let _ = client.exact_binary();
}
