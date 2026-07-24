use concord::api;
use concord::body::{Stream, StreamBody};
use concord::codec::OctetStream;
use concord::Bytes;

api! {
    client UploadApi {
        base "http://upload.example.test/";
    }

    POST upload(#[body] body: Stream<OctetStream>) -> Bytes;
}

fn main() {
    let endpoint = upload_api::Upload::new(StreamBody::from_bytes(Bytes::new()));
    let _ = endpoint.clone();
}
