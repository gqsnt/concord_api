use concord::{Bytes, OctetStream, Stream, StreamBody, api};

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
