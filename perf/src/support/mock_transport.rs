use crate::support::mock_body::{ChunkedBodyStream, chunked_bytes};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacementPlan, DynBody, RequestExecutionContext, RequestMeta, Transport, TransportError,
};
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
}

#[derive(Clone)]
pub struct RecordedRequest {
    pub meta: RequestMeta,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: RecordedBody,
    pub timeout: Option<std::time::Duration>,
    pub auth_plan: AuthPlacementPlan,
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
            .field("auth_plan_present", &auth_plan_present(&self.auth_plan))
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

    fn into_response(self) -> http::Response<DynBody> {
        let content_length = Some(self.body_len() as u64);
        let body = match self.body {
            MockResponseBody::Empty => DynBody::empty(),
            MockResponseBody::Bytes(bytes) => DynBody::from_bytes(bytes),
            MockResponseBody::Chunks(chunks) => {
                DynBody::from_byte_stream(ChunkedBodyStream::new(chunks))
            }
        };
        let mut response = http::Response::new(body);
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers;
        if let Some(length) = content_length {
            response.headers_mut().insert(
                http::header::CONTENT_LENGTH,
                HeaderValue::from_str(&length.to_string()).expect("length"),
            );
        }
        response
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
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let state = self.state.clone();
        Box::pin(async move {
            use http_body_util::BodyExt as _;
            let (parts, body) = req.into_parts();
            let context = parts
                .extensions
                .get::<RequestExecutionContext>()
                .cloned()
                .expect("context");
            let auth_plan = parts
                .extensions
                .get::<AuthPlacementPlan>()
                .cloned()
                .unwrap_or_default();
            let url = parts.uri.to_string().parse().expect("URL");
            let bytes = body
                .collect()
                .await
                .map_err(TransportError::new)?
                .to_bytes();
            let recorded_body = if bytes.is_empty() {
                RecordedBody::Empty
            } else {
                RecordedBody::Bytes { len: bytes.len() }
            };
            let recorded = RecordedRequest {
                meta: context.meta,
                url,
                headers: parts.headers,
                body: recorded_body,
                timeout: context.timeout,
                auth_plan,
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

            Ok(response.into_response())
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

fn auth_plan_present(plan: &AuthPlacementPlan) -> bool {
    !plan.sensitive_query_keys.is_empty() || !plan.slots.is_empty()
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
            auth_plan: concord_core::advanced::AuthPlacementPlan {
                sensitive_query_keys: vec!["token".to_string()],
                slots: Vec::new(),
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
        let body = response.into_response().into_body();

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test runtime");
        let bytes = rt.block_on(async {
            http_body_util::BodyExt::collect(body)
                .await
                .expect("empty body")
                .to_bytes()
        });
        assert!(bytes.is_empty());
    }
}
