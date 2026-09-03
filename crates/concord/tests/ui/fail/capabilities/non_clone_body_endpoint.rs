use concord::api;
use concord::body::{Stream, StreamBody};
use concord::codec::OctetStream;
use concord::Bytes;

api! {
    client UploadApi {
        timeout off;
        base "http://upload.example.test/";


    POST upload(body body: ::concord::body::StreamBody using Stream<OctetStream>) -> Bytes using Bytes;
}}

fn main() {
    let endpoint = UploadApi::unbound().upload(StreamBody::from_bytes(Bytes::new()));
    let _ = endpoint.clone();
}
