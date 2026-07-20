use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use concord_core::advanced::{
    BodyError, BodyErrorKind, BodyFactory, BodyKind, Bytes, HeaderValue, RequestBody, StreamBody,
};
use futures_core::Stream;

struct DownstreamStream {
    item: Option<Result<Bytes, io::Error>>,
}

impl Stream for DownstreamStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.item.take())
    }
}

struct DownstreamFactory;

impl BodyFactory for DownstreamFactory {
    fn content_type(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static("application/x-downstream-body"))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        Ok(StreamBody::from_bytes(Bytes::from_static(b"fresh-body")))
    }
}

#[test]
fn downstream_body_contracts_need_only_the_advanced_public_surface() {
    fn assert_send<T: Send>(_: &T) {}
    fn assert_sync<T: Sync>(_: &T) {}

    let stream = StreamBody::from_stream(DownstreamStream {
        item: Some(Ok(Bytes::from_static(b"downstream-body-secret-851a"))),
    })
    .exact_length(27);
    assert_send(&stream);
    assert!(!format!("{stream:?}").contains("downstream-body-secret-851a"));

    let reader = StreamBody::from_reader(std::io::Cursor::new(b"reader".to_vec()));
    assert_send(&reader);

    let bytes = RequestBody::bytes(Bytes::from_static(b"bytes"));
    assert_eq!(bytes.kind(), BodyKind::Buffered);
    let stream = RequestBody::stream(stream);
    assert_eq!(stream.kind(), BodyKind::Stream);
    let factory = RequestBody::factory(DownstreamFactory);
    assert_eq!(factory.kind(), BodyKind::Stream);
    assert_send(&factory);
    assert_sync(&DownstreamFactory);

    let error =
        BodyError::with_safe_message(BodyErrorKind::Factory, "safe downstream body configuration");
    assert_eq!(error.kind(), BodyErrorKind::Factory);
    assert!(
        error
            .to_string()
            .contains("safe downstream body configuration")
    );
}

#[tokio::test]
async fn file_constructor_is_public_and_sanitizes_native_failures() {
    let missing = std::env::temp_dir().join(format!(
        "concord-request-body-missing-file-{}.tmp",
        std::process::id()
    ));
    let error = StreamBody::from_file(&missing)
        .await
        .expect_err("missing downstream file");
    assert_eq!(error.kind(), BodyErrorKind::Producer);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains(&missing.to_string_lossy().to_string()));
    }
}
