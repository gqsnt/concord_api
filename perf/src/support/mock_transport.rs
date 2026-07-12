use crate::support::mock_body::{EmptyBody, FixedBody, chunked_bytes};
use bytes::Bytes;
use concord_core::advanced::{
    RequestMeta, Transport, TransportBody, TransportError, TransportRequest, TransportRequestBody,
    TransportResponse,
};
use concord_core::auth::RequestExtensions;
use http::{HeaderMap, HeaderValue, StatusCode};
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct MockTransport {
    state: Arc<Mutex<MockTransportState>>,
}

struct MockTransportState {
    scripted: VecDeque<MockResponse>,
    repeat: Option<MockResponse>,
    requests: Vec<RecordedRequest>,
}

#[derive(Clone)]
pub struct MockResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    body: MockResponseBody,
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
    Stream,
}

#[derive(Clone)]
pub struct RecordedRequest {
    pub meta: RequestMeta,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: RecordedBody,
    pub timeout: Option<std::time::Duration>,
    pub extensions: RequestExtensions,
}

impl fmt::Debug for MockResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("body_len", &self.body_len())
            .finish()
    }
}

impl fmt::Debug for RecordedRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordedRequest")
            .field("meta", &self.meta)
            .field("url", &safe_url_for_debug(&self.url))
            .field("header_count", &self.headers.len())
            .field("body", &self.body)
            .field("timeout", &self.timeout)
            .field("extensions_present", &extensions_present(&self.extensions))
            .finish()
    }
}

impl fmt::Debug for MockTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.lock().expect("mock transport poisoned");
        f.debug_struct("MockTransport")
            .field("scripted", &state.scripted.len())
            .field("repeat", &state.repeat.is_some())
            .field("requests", &state.requests.len())
            .finish()
    }
}

impl MockResponse {
    pub fn empty(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: MockResponseBody::Empty,
        }
    }

    pub fn bytes(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: MockResponseBody::Bytes(body.into()),
        }
    }

    pub fn chunked(status: StatusCode, chunks: impl IntoIterator<Item = Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: MockResponseBody::Chunks(chunks.into_iter().collect()),
        }
    }

    pub fn text(status: StatusCode, body: impl Into<Bytes>) -> Self {
        let mut response = Self::bytes(status, body);
        response.headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );
        response
    }

    pub fn with_header(mut self, name: http::header::HeaderName, value: HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn with_json_header(mut self) -> Self {
        self.headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        self
    }

    fn body_len(&self) -> usize {
        match &self.body {
            MockResponseBody::Empty => 0,
            MockResponseBody::Bytes(bytes) => bytes.len(),
            MockResponseBody::Chunks(chunks) => chunks.iter().map(Bytes::len).sum(),
        }
    }

    fn into_parts(self) -> (StatusCode, HeaderMap, Box<dyn TransportBody>, Option<u64>) {
        let content_length = Some(self.body_len() as u64);
        let body: Box<dyn TransportBody> = match self.body {
            MockResponseBody::Empty => Box::new(EmptyBody),
            MockResponseBody::Bytes(bytes) => Box::new(FixedBody::new(bytes)),
            MockResponseBody::Chunks(chunks) => Box::new(ChunkedTransportBody { chunks }),
        };
        (self.status, self.headers, body, content_length)
    }
}

struct ChunkedTransportBody {
    chunks: VecDeque<Bytes>,
}

impl TransportBody for ChunkedTransportBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.chunks.pop_front()) })
    }
}

impl MockTransport {
    pub fn repeating(response: MockResponse) -> Self {
        Self {
            state: Arc::new(Mutex::new(MockTransportState {
                scripted: VecDeque::new(),
                repeat: Some(response),
                requests: Vec::new(),
            })),
        }
    }

    pub fn scripted(responses: impl IntoIterator<Item = MockResponse>) -> Self {
        Self {
            state: Arc::new(Mutex::new(MockTransportState {
                scripted: responses.into_iter().collect(),
                repeat: None,
                requests: Vec::new(),
            })),
        }
    }

    pub fn push_response(&self, response: MockResponse) {
        self.state
            .lock()
            .expect("mock transport poisoned")
            .scripted
            .push_back(response);
    }

    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.state
            .lock()
            .expect("mock transport poisoned")
            .requests
            .clone()
    }
}

impl Transport for MockTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let state = self.state.clone();
        Box::pin(async move {
            let TransportRequest {
                meta,
                url,
                headers,
                body,
                timeout,
                rate_limit,
                extensions,
            } = req;
            let recorded = RecordedRequest {
                meta: meta.clone(),
                url: url.clone(),
                headers: headers.clone(),
                body: match &body {
                    TransportRequestBody::Empty => RecordedBody::Empty,
                    TransportRequestBody::Bytes(bytes) => RecordedBody::Bytes { len: bytes.len() },
                    TransportRequestBody::Stream(_) => RecordedBody::Stream,
                },
                timeout,
                extensions: extensions.clone(),
            };

            let response = {
                let mut state = state.lock().expect("mock transport poisoned");
                state.requests.push(recorded);
                if let Some(response) = state.scripted.pop_front() {
                    response
                } else if let Some(response) = &state.repeat {
                    response.clone()
                } else {
                    panic!("mock transport exhausted");
                }
            };

            let (status, headers, body, content_length) = response.into_parts();

            Ok(TransportResponse {
                meta,
                url,
                status,
                headers,
                content_length,
                rate_limit,
                body,
            })
        })
    }
}

pub fn repeating_text(status: StatusCode, body: impl Into<Bytes>) -> MockTransport {
    MockTransport::repeating(MockResponse::text(status, body))
}

pub fn repeating_chunked(status: StatusCode, payload: Bytes, chunk_size: usize) -> MockTransport {
    MockTransport::repeating(MockResponse::chunked(
        status,
        chunked_bytes(payload, chunk_size),
    ))
}

fn safe_url_for_debug(url: &url::Url) -> String {
    match url.host_str() {
        Some(host) => format!("{}://{}{}", url.scheme(), host, url.path()),
        None => format!("{}://<no-host>{}", url.scheme(), url.path()),
    }
}

fn extensions_present(ext: &RequestExtensions) -> bool {
    !ext.auth_plan.sensitive_query_keys.is_empty() || !ext.auth_plan.slots.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::{AUTHORIZATION, HeaderName, HeaderValue};

    #[test]
    fn recorded_request_debug_is_redacted() {
        let request = RecordedRequest {
            meta: RequestMeta {
                endpoint: "perf_debug",
                method: http::Method::GET,
                idempotent: true,
                attempt: 3,
                page_index: 1,
            },
            url: url::Url::parse("https://example.com/path?token=SECRET_QUERY_VALUE").expect("url"),
            headers: {
                let mut headers = HeaderMap::new();
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_static("Bearer SECRET_HEADER"),
                );
                headers.insert(
                    HeaderName::from_static("x-test"),
                    HeaderValue::from_static("value"),
                );
                headers
            },
            body: RecordedBody::Bytes { len: 16 },
            timeout: Some(std::time::Duration::from_secs(1)),
            extensions: RequestExtensions {
                auth_plan: concord_core::advanced::AuthPlacementPlan {
                    sensitive_query_keys: vec!["token".to_string()],
                    slots: Vec::new(),
                },
            },
        };

        let debug = format!("{request:?}");
        assert!(!debug.contains("SECRET_QUERY_VALUE"));
        assert!(!debug.contains("SECRET_HEADER"));
        assert!(!debug.contains("token="));
        assert!(debug.contains("header_count"));
    }

    #[test]
    fn mock_response_debug_is_redacted() {
        let response = MockResponse::text(StatusCode::OK, "secret body");
        let debug = format!("{response:?}");
        assert!(!debug.contains("secret body"));
        assert!(!debug.contains("text/plain"));
        assert!(debug.contains("header_count"));
        assert!(debug.contains("body_len"));
    }

    #[test]
    fn empty_response_body_is_eof() {
        let response = MockResponse::empty(StatusCode::NO_CONTENT);
        let (_, _, mut body, _) = response.into_parts();

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test runtime");
        let chunk = rt.block_on(async { body.next_chunk().await.expect("empty body") });
        assert!(chunk.is_none());
    }
}
