// V3-L11 / VALUE-05: requests and intents whose declaration contains a body
// slot are not `Clone` — even when the payload type is cloneable or the
// optional body is currently absent. Body-free requests and intents are
// `Clone` without requiring a cloneable output.
use concord::api;
use concord::body::RequestStream;
use concord::{Binary, Bytes};

api! {
    client UploadApi {
        timeout 30s;
        base "http://upload.example.test/";

        POST upload(body body: ::concord::body::RequestStream via Binary) -> Bytes via Binary;
        PUT store(body body?: Bytes via Binary) -> Bytes via Binary;
        GET read() -> Bytes via Binary;
    }
}

fn main() {
    let client = UploadApi::new().unwrap();
    let _ = client.read().clone();
    let _ = UploadApi::unbound().read().clone();

    let endpoint = UploadApi::unbound().upload(RequestStream::from_bytes(Bytes::new()));
    let _ = endpoint.clone();
    let _ = client.upload(RequestStream::from_bytes(Bytes::new())).clone();
    let _ = UploadApi::unbound().store().clone();
    let _ = client.store().clone();
}
