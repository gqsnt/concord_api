use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use concord_core::contracts::{
    Binary, BodyError, BodyErrorKind, BodyFactory, BodyKind, Bytes, MAX_EMPTY_REQUEST_CHUNKS,
    PageRequestRepresentation, PreparedBody, RequestStream, Text,
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
    fn create(&self) -> Result<RequestStream, BodyError> {
        Ok(RequestStream::from_bytes(Bytes::from_static(b"fresh-body")))
    }
}

#[test]
fn downstream_body_contracts_need_only_the_contract_surface() {
    fn assert_send<T: Send>(_: &T) {}
    fn assert_sync<T: Sync>(_: &T) {}

    let stream = RequestStream::from_stream(DownstreamStream {
        item: Some(Ok(Bytes::from_static(b"downstream-body-secret-851a"))),
    })
    .exact_length(27);
    assert_send(&stream);
    assert!(!format!("{stream:?}").contains("downstream-body-secret-851a"));

    let reader = RequestStream::from_reader(std::io::Cursor::new(b"reader".to_vec()));
    assert_send(&reader);

    let bytes = PreparedBody::bytes(Bytes::from_static(b"bytes"));
    assert_eq!(bytes.kind(), BodyKind::Buffered);
    let stream = PreparedBody::stream(stream);
    assert_eq!(stream.kind(), BodyKind::Stream);
    let factory = PreparedBody::factory(DownstreamFactory)
        .content_type("application/x-downstream-body")
        .expect("factory media type");
    assert_eq!(factory.kind(), BodyKind::Stream);
    assert_eq!(factory.media_type(), Some("application/x-downstream-body"));
    assert!(factory.is_replayable());
    assert_send(&factory);
    assert_sync(&DownstreamFactory);
    assert_eq!(MAX_EMPTY_REQUEST_CHUNKS, 1_024);

    let error =
        BodyError::with_safe_message(BodyErrorKind::Factory, "safe downstream body configuration");
    assert_eq!(error.kind(), BodyErrorKind::Factory);
    assert!(
        error
            .to_string()
            .contains("safe downstream body configuration")
    );

    fn assert_page_body<B, Input>()
    where
        B: PageRequestRepresentation<Input>,
        Input: Clone + Send + 'static,
    {
    }
    assert_page_body::<Binary, Bytes>();
    assert_page_body::<Text, String>();
}

#[tokio::test]
async fn file_constructor_is_public_and_sanitizes_native_failures() {
    let missing = std::env::temp_dir().join(format!(
        "concord-request-body-missing-file-{}.tmp",
        std::process::id()
    ));
    let error = RequestStream::from_file(&missing)
        .await
        .expect_err("missing downstream file");
    assert_eq!(error.kind(), BodyErrorKind::Producer);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains(&missing.to_string_lossy().to_string()));
    }
}
