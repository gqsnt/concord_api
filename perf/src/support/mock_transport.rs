use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

#[path = "../../../concord_test_support/src/mock.rs"]
mod native_mock;

#[derive(Clone)]
pub struct MockTransport {
    server: native_mock::MockServer,
    handle: Arc<Mutex<native_mock::MockHandle>>,
}

#[derive(Clone)]
pub struct MockResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    body: MockResponseBody,
    disconnect: bool,
}

#[derive(Clone)]
enum MockResponseBody {
    Empty,
    Bytes(Bytes),
    Chunks(VecDeque<Bytes>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordedBody {
    Empty,
    Bytes { len: usize },
}

#[derive(Clone)]
pub struct RecordedRequest {
    pub endpoint: Option<String>,
    pub method: Method,
    pub attempt: Option<u32>,
    pub page_index: Option<u32>,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: RecordedBody,
    pub timeout: Option<std::time::Duration>,
}

impl fmt::Debug for MockResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("body_len", &self.body_len())
            .finish()
    }
}

impl fmt::Debug for RecordedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordedRequest")
            .field("endpoint", &self.endpoint)
            .field("method", &self.method)
            .field("attempt", &self.attempt)
            .field("page_index", &self.page_index)
            .field("url", &"<redacted>")
            .field(
                "headers",
                &concord_core::advanced::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &self.body)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl fmt::Debug for MockTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockTransport")
            .field(
                "requests",
                &self.handle.lock().expect("mock handle").recorded_len(),
            )
            .finish()
    }
}

impl MockResponse {
    pub fn empty(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: MockResponseBody::Empty,
            disconnect: false,
        }
    }

    pub fn bytes(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: MockResponseBody::Bytes(body.into()),
            disconnect: false,
        }
    }

    pub fn chunked(status: StatusCode, chunks: impl IntoIterator<Item = Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: MockResponseBody::Chunks(chunks.into_iter().collect()),
            disconnect: false,
        }
    }

    pub fn text(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self::bytes(status, body).with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        )
    }

    pub fn with_header(mut self, name: http::HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn with_json_header(self) -> Self {
        self.with_header(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )
    }

    fn body_len(&self) -> usize {
        match &self.body {
            MockResponseBody::Empty => 0,
            MockResponseBody::Bytes(bytes) => bytes.len(),
            MockResponseBody::Chunks(chunks) => chunks.iter().map(Bytes::len).sum(),
        }
    }

    fn into_reply(self) -> native_mock::MockReply {
        if self.disconnect {
            return native_mock::MockReply::disconnect_after_request();
        }
        let mut reply = native_mock::MockReply::status(self.status);
        for (name, value) in self.headers {
            if let Some(name) = name {
                reply = reply.with_header(name, value);
            }
        }
        match self.body {
            MockResponseBody::Empty => reply,
            MockResponseBody::Bytes(bytes) => reply.with_body(bytes),
            MockResponseBody::Chunks(chunks) => reply.with_chunks(chunks),
        }
    }
}

impl MockTransport {
    pub fn failing() -> Self {
        let response = MockResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            headers: HeaderMap::new(),
            body: MockResponseBody::Empty,
            disconnect: true,
        };
        Self::repeating(response)
    }
    pub fn repeating(response: MockResponse) -> Self {
        let (server, handle) = native_mock::mock().repeating(response.into_reply()).build();
        Self {
            server,
            handle: Arc::new(Mutex::new(handle)),
        }
    }

    pub fn scripted(responses: impl IntoIterator<Item = MockResponse>) -> Self {
        let (server, handle) = native_mock::mock()
            .replies(responses.into_iter().map(MockResponse::into_reply))
            .build();
        Self {
            server,
            handle: Arc::new(Mutex::new(handle)),
        }
    }

    pub fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.server.configure_reqwest(builder)
    }

    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.handle
            .lock()
            .expect("mock handle")
            .recorded()
            .into_iter()
            .map(|request| {
                let body = if request.body.is_empty() {
                    RecordedBody::Empty
                } else {
                    RecordedBody::Bytes {
                        len: request.body.len(),
                    }
                };
                RecordedRequest {
                    endpoint: request.endpoint,
                    method: request.method,
                    attempt: request.attempt,
                    page_index: request.page_index,
                    url: request.url,
                    headers: request.headers,
                    body,
                    timeout: request.timeout,
                }
            })
            .collect()
    }
}

pub fn repeating_text(status: StatusCode, body: impl Into<Bytes>) -> MockTransport {
    MockTransport::repeating(MockResponse::text(status, body))
}

pub fn repeating_chunked(status: StatusCode, payload: Bytes, chunk_size: usize) -> MockTransport {
    MockTransport::repeating(MockResponse::chunked(
        status,
        crate::support::mock_body::chunked_bytes(payload, chunk_size),
    ))
}
