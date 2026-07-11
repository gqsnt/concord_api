use bytes::Bytes;
use concord_core::advanced::{
    MultipartBody, Transport, TransportBody, TransportError, TransportRequest,
    TransportRequestBody, TransportResponse,
};
use concord_core::prelude::Text;
use concord_macros::api;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, StatusCode};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

api! {
    client MultipartRequestApi { base "https://example.com" }
    POST Upload(body: Multipart<()>)
        path ["upload"]
        -> Text<String>
}

#[derive(Clone)]
struct RecordingTransport(Arc<Mutex<Option<(String, Bytes)>>>);

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let recorded = self.0.clone();
        Box::pin(async move {
            let content_type = req
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let body = match req.body {
                TransportRequestBody::Stream(mut stream) => {
                    let mut body = Vec::new();
                    while let Some(chunk) =
                        std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await
                    {
                        body.extend_from_slice(&chunk?);
                    }
                    Bytes::from(body)
                }
                _ => panic!("multipart request must use a stream body"),
            };
            *recorded.lock().expect("recording lock") = Some((content_type, body));
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            );
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: StatusCode::OK,
                headers,
                content_length: Some(2),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody(Some(Bytes::from_static(b"ok")))),
            })
        })
    }
}

struct StaticBody(Option<Bytes>);

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.take()) })
    }
}

#[tokio::test]
async fn generated_multipart_form_data_request_reaches_transport() {
    let recorded = Arc::new(Mutex::new(None));
    let api = multipart_request_api::MultipartRequestApi::new_with_transport(RecordingTransport(
        recorded.clone(),
    ));

    let response = api
        .upload(
            MultipartBody::new()
                .text("title", "hello")
                .bytes("file", Bytes::from_static(b"abc")),
        )
        .execute()
        .await
        .expect("multipart request succeeds");
    assert_eq!(response, "ok");

    let (content_type, body) = recorded
        .lock()
        .expect("recording lock")
        .clone()
        .expect("request recorded");
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    let rendered = String::from_utf8(body.to_vec()).expect("multipart is UTF-8 here");
    assert!(rendered.contains("Content-Disposition:"));
    assert!(rendered.contains("hello"));
    assert!(rendered.contains("abc"));
}
