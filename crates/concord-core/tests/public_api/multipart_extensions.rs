#![cfg(feature = "multipart")]

use concord_core::contracts::{
    BodyError, BodyKind, Bytes, MultipartBody, MultipartFactory, MultipartPart, RequestBody,
    StreamBody,
};

struct DownstreamMultipartFactory;

impl MultipartFactory for DownstreamMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        MultipartBody::new()
            .part("text", MultipartPart::text("downstream text")?)?
            .part(
                "bytes",
                MultipartPart::bytes(Bytes::from_static(b"downstream bytes"))?,
            )
    }
}

#[test]
fn downstream_multipart_contract_uses_only_contract_imports() {
    fn assert_send<T: Send>(_: &T) {}
    fn assert_sync<T: Sync>(_: &T) {}

    let streamed = MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(b"stream")))
        .file_name("public-name.bin")
        .expect("valid filename")
        .content_type("application/octet-stream")
        .expect("valid content type");
    let body = MultipartBody::new()
        .part("stream", streamed)
        .expect("valid part");
    assert_send(&body);
    let request_body = RequestBody::multipart(body);
    assert_eq!(request_body.kind(), BodyKind::Multipart);
    assert_send(&request_body);

    let factory_body = RequestBody::multipart_factory(DownstreamMultipartFactory);
    assert_eq!(factory_body.kind(), BodyKind::Multipart);
    assert_sync(&DownstreamMultipartFactory);
    assert!(!format!("{factory_body:?}").contains("downstream text"));
}
