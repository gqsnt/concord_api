use bytes::Bytes;
use concord_core::advanced::{DynBody, RequestExecutionContext, Transport, TransportError};
use concord_core::error::ErrorCategory;
use concord_core::prelude::ApiClientError;
use concord_core::transport::RequestMeta;
use concord_macros::api;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

mod bytes_response_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client BytesResponseApi {
            base "https://example.com"
        }

        GET Download
            path ["download"]
            -> Bytes
    }

    pub(super) use bytes_response_api::BytesResponseApi;
}

use bytes_response_contract::BytesResponseApi;

#[derive(Clone)]
struct RecordingBytesTransport {
    requests: Arc<StdMutex<Vec<CapturedRequest>>>,
    fixture: ResponseFixture,
    send_count: Arc<AtomicUsize>,
}

#[derive(Debug, Clone)]
struct CapturedRequest {
    meta: RequestMeta,
    method: Method,
    accept: Option<String>,
}

#[derive(Clone)]
enum ResponseFixture {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        chunks: Vec<Bytes>,
        content_length: Option<u64>,
        polls: Arc<AtomicUsize>,
    },
}

impl RecordingBytesTransport {
    fn new(fixture: ResponseFixture) -> Self {
        Self {
            requests: Arc::new(StdMutex::new(Vec::new())),
            fixture,
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn requests(&self) -> Vec<CapturedRequest> {
        self.requests.lock().expect("request lock").clone()
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }
}

impl Transport for RecordingBytesTransport {
    fn send(
        &self,
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        self.send_count.fetch_add(1, Ordering::SeqCst);
        let accept = req
            .headers()
            .get(http::header::ACCEPT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        self.requests
            .lock()
            .expect("request lock")
            .push(CapturedRequest {
                meta: req
                    .extensions()
                    .get::<RequestExecutionContext>()
                    .expect("context")
                    .meta
                    .clone(),
                method: req.method().clone(),
                accept,
            });

        let fixture = self.fixture.clone();
        Box::pin(async move {
            match fixture {
                ResponseFixture::Buffered {
                    status,
                    headers,
                    chunks,
                    content_length,
                    polls,
                } => {
                    let mut response = http::Response::new(DynBody::from_byte_stream(
                        PollingBody::new(chunks, polls),
                    ));
                    *response.status_mut() = status;
                    *response.headers_mut() = headers;
                    if let Some(length) = content_length {
                        response.headers_mut().insert(
                            http::header::CONTENT_LENGTH,
                            HeaderValue::from_str(&length.to_string()).expect("length"),
                        );
                    }
                    Ok(response)
                }
            }
        })
    }
}

struct PollingBody {
    chunks: VecDeque<Bytes>,
    polls: Arc<AtomicUsize>,
}

impl PollingBody {
    fn new(chunks: Vec<Bytes>, polls: Arc<AtomicUsize>) -> Self {
        Self {
            chunks: chunks.into(),
            polls,
        }
    }
}

impl futures_core::Stream for PollingBody {
    type Item = Result<Bytes, concord_core::advanced::BodyError>;
    fn poll_next(
        mut self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let chunk = self.chunks.pop_front();
        if chunk.is_some() {
            self.polls.fetch_add(1, Ordering::SeqCst);
        }
        std::task::Poll::Ready(chunk.map(Ok))
    }
}

fn buffered_fixture(status: StatusCode, chunks: Vec<Bytes>) -> (ResponseFixture, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    (
        ResponseFixture::Buffered {
            status,
            headers: HeaderMap::new(),
            chunks,
            content_length: None,
            polls: polls.clone(),
        },
        polls,
    )
}

#[tokio::test]
async fn generated_bytes_response_reads_body_without_accept_header() {
    let (fixture, polls) = buffered_fixture(
        StatusCode::OK,
        vec![Bytes::from_static(b"hel"), Bytes::from_static(b"lo")],
    );
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_transport(transport.clone());

    let response = api
        .download()
        .execute()
        .await
        .expect("bytes download succeeds");
    assert_eq!(response, Bytes::from_static(b"hello"));

    assert_eq!(transport.send_count(), 1);
    let requests = transport.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta.endpoint, "Download");
    assert_eq!(requests[0].meta.method, Method::GET);
    assert_eq!(requests[0].method, Method::GET);
    assert_eq!(requests[0].accept, None);
    assert!(polls.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn generated_bytes_limit_failure_is_body_limited() {
    let (fixture, polls) = buffered_fixture(
        StatusCode::OK,
        vec![Bytes::from_static(b"abcd"), Bytes::from_static(b"efgh")],
    );
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_transport(transport.clone()).configure(|cfg| {
        cfg.max_response_body_bytes(4);
    });

    let err = api
        .download()
        .execute()
        .await
        .expect_err("bytes response over the limit should fail");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert_eq!(transport.send_count(), 1);
    assert!(polls.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn generated_bytes_status_failure_is_body_free() {
    let sentinel = Bytes::from_static(b"SECRET_BYTES_STATUS_SENTINEL_MUST_NOT_APPEAR");
    let (fixture, polls) =
        buffered_fixture(StatusCode::INTERNAL_SERVER_ERROR, vec![sentinel.clone()]);
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_transport(transport.clone());

    let err = api
        .download()
        .execute()
        .await
        .expect_err("status failure should not decode bytes");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "Download");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(transport.send_count(), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let rendered = format!("{err:?}");
    assert!(!rendered.contains("SECRET_BYTES_STATUS_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn generated_bytes_response_includes_metadata_and_value() {
    let polls = Arc::new(AtomicUsize::new(0));
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::HeaderName::from_static("x-response-id"),
        HeaderValue::from_static("abc123"),
    );
    let fixture = ResponseFixture::Buffered {
        status: StatusCode::CREATED,
        headers,
        chunks: vec![Bytes::from_static(b"hello")],
        content_length: Some(5),
        polls: polls.clone(),
    };
    let transport = RecordingBytesTransport::new(fixture);
    let api = BytesResponseApi::new_with_transport(transport.clone());

    let response = api
        .download()
        .response()
        .await
        .expect("bytes response succeeds");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response
            .headers()
            .get(http::header::HeaderName::from_static("x-response-id"))
            .and_then(|value| value.to_str().ok()),
        Some("abc123")
    );
    assert_eq!(response.meta().endpoint, "Download");
    assert_eq!(response.meta().method, Method::GET);
    assert_eq!(response.into_value(), Bytes::from_static(b"hello"));
    assert_eq!(transport.send_count(), 1);
    assert!(polls.load(Ordering::SeqCst) > 0);
}
