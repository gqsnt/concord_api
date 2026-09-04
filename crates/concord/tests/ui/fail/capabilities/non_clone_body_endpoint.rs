use concord::api;
use concord::body::RequestStream;
use concord::{Binary, Bytes};

api! {
    client UploadApi {
        timeout 30s;
        base "http://upload.example.test/";


    POST upload(body body: ::concord::body::RequestStream via Binary) -> Bytes via Binary;
}}

fn main() {
    let endpoint = UploadApi::unbound().upload(RequestStream::from_bytes(Bytes::new()));
    let _ = endpoint.clone();
}
