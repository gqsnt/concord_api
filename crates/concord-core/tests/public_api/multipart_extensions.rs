#![cfg(feature = "multipart")]

use concord_core::contracts::{
    BodyError, BodyKind, Bytes, MultipartBody, MultipartFactory, MultipartPart, PreparedBody,
    RequestStream,
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

    let streamed = MultipartPart::stream(RequestStream::from_bytes(Bytes::from_static(b"stream")))
        .file_name("public-name.bin")
        .expect("valid filename")
        .content_type("application/octet-stream")
        .expect("valid content type");
    let body = MultipartBody::new()
        .part("stream", streamed)
        .expect("valid part");
    assert_send(&body);
    let request_body = PreparedBody::multipart(body).expect("prepared multipart body");
    assert_eq!(request_body.kind(), BodyKind::Multipart);
    assert!(!request_body.is_replayable());
    assert!(
        request_body
            .media_type()
            .is_some_and(|media| media.starts_with("multipart/form-data; boundary="))
    );
    assert_send(&request_body);

    let factory_body = PreparedBody::multipart_factory(DownstreamMultipartFactory)
        .expect("prepared multipart factory");
    assert_eq!(factory_body.kind(), BodyKind::Multipart);
    assert_sync(&DownstreamMultipartFactory);
    assert!(!format!("{factory_body:?}").contains("downstream text"));
}
