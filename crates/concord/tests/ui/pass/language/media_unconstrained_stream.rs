use concord::api;
use concord::body::{Stream, StreamBody};
use concord::codec::{MediaType, OctetStream};
use concord::prelude::AnyMedia;
use concord::Bytes;

pub struct VendorMedia;

impl MediaType for VendorMedia {
    const MEDIA_TYPE: &'static str = "application/vnd.example.stream";
}

api! {
    client StreamApi {
        base "http://stream.example.test/";
    }

    POST upload(#[body] body: Stream) -> Bytes;
    GET download() -> Stream;
    GET explicit_any() -> Stream<AnyMedia>;
    GET exact_binary() -> Stream<OctetStream>;
    GET exact_vendor() -> Stream<VendorMedia>;
}

fn main() {
    let client = StreamApi::new().unwrap();
    let _ = client.upload(StreamBody::from_bytes(Bytes::from_static(b"body")));
    let _ = client.download();
    let _ = client.explicit_any();
    let _ = client.exact_binary();
    let _ = client.exact_vendor();
}
