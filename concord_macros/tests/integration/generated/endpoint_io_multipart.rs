use bytes::Bytes;
use concord_core::advanced::{DynBody, MultipartBody, Transport, TransportError};
use concord_core::prelude::Text;
use concord_macros::api;
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
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let recorded = self.0.clone();
        Box::pin(async move {
            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let body = http_body_util::BodyExt::collect(req.into_body())
                .await
                .map_err(TransportError::new)?
                .to_bytes();
            *recorded.lock().expect("recording lock") = Some((content_type, body));
            let mut headers = HeaderMap::new();
            headers.insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            );
            let mut response = http::Response::new(DynBody::from_bytes(Bytes::from_static(b"ok")));
            *response.status_mut() = StatusCode::OK;
            *response.headers_mut() = headers;
            Ok(response)
        })
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
