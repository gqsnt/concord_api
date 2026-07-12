use bytes::Bytes;
use concord_core::transport::*;
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum RecordedBody {
    Empty,
    Bytes(Bytes),
}

impl RecordedBody {
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Empty => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub meta: RequestMeta,
    pub url: url::Url,
    pub headers: http::HeaderMap,
    pub body: RecordedBody,
    pub timeout: Option<Duration>,
}

#[derive(Clone, Debug)]
pub struct MockReply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl MockReply {
    pub fn ok_json(body: Bytes) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        Self {
            status: StatusCode::OK,
            headers,
            body,
        }
    }

    pub fn ok_text(body: Bytes) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/plain"),
        );
        Self {
            status: StatusCode::OK,
            headers,
            body,
        }
    }

    pub fn status(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    pub fn with_header(mut self, name: http::header::HeaderName, value: http::HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn with_body(mut self, body: Bytes) -> Self {
        self.body = body;
        self
    }
}

#[derive(Debug)]
struct MockState {
    recorded: Mutex<Vec<RecordedRequest>>,
    replies: Mutex<VecDeque<MockReply>>,
}

#[derive(Clone)]
pub struct MockTransport {
    st: Arc<MockState>,
}

pub struct MockHandle {
    st: Arc<MockState>,
    finished: bool,
}

pub struct MockBuilder {
    replies: Vec<MockReply>,
}

impl Default for MockBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBuilder {
    pub fn new() -> Self {
        Self {
            replies: Vec::new(),
        }
    }

    pub fn reply(mut self, r: MockReply) -> Self {
        self.replies.push(r);
        self
    }

    pub fn replies(mut self, rs: impl IntoIterator<Item = MockReply>) -> Self {
        self.replies.extend(rs);
        self
    }

    pub fn build(self) -> (MockTransport, MockHandle) {
        let st = Arc::new(MockState {
            recorded: Mutex::new(Vec::new()),
            replies: Mutex::new(self.replies.into_iter().collect()),
        });
        (
            MockTransport { st: st.clone() },
            MockHandle {
                st,
                finished: false,
            },
        )
    }
}

pub fn mock() -> MockBuilder {
    MockBuilder::new()
}

impl MockHandle {
    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.st.recorded.lock().unwrap().clone()
    }

    pub fn recorded_len(&self) -> usize {
        self.st.recorded.lock().unwrap().len()
    }

    pub fn assert_recorded_len(&self, expected: usize) {
        let got = self.recorded_len();
        if got != expected {
            let reqs = self.recorded();
            panic!(
                "recorded request count mismatch\n  expected: {expected}\n  got: {got}\n  recorded:\n{:#?}",
                reqs
            );
        }
    }

    pub fn remaining_replies(&self) -> usize {
        self.st.replies.lock().unwrap().len()
    }

    pub fn assert_no_remaining_replies(&self) {
        let left = self.remaining_replies();
        if left != 0 {
            panic!("mock replies not fully consumed: remaining={left}");
        }
    }

    pub fn finish(mut self) {
        self.assert_no_remaining_replies();
        self.finished = true;
    }
}

impl Drop for MockHandle {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if std::thread::panicking() {
            return;
        }
        let left = self.st.replies.lock().unwrap().len();
        if left != 0 {
            panic!("mock replies not fully consumed (drop): remaining={left}");
        }
    }
}

impl concord_core::advanced::Transport for MockTransport {
    fn send(
        &self,
        req: http::Request<concord_core::advanced::DynBody>,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        http::Response<concord_core::advanced::DynBody>,
                        TransportError,
                    >,
                > + Send,
        >,
    > {
        let st = self.st.clone();
        Box::pin(async move {
            use http_body_util::BodyExt as _;
            let (parts, body) = req.into_parts();
            let context = parts
                .extensions
                .get::<RequestExecutionContext>()
                .cloned()
                .expect("transport request context");
            let url = parts.uri.to_string().parse().expect("absolute request URI");
            let bytes = body
                .collect()
                .await
                .map_err(TransportError::new)?
                .to_bytes();
            let recorded_body = if bytes.is_empty() {
                RecordedBody::Empty
            } else {
                RecordedBody::Bytes(bytes)
            };
            st.recorded.lock().unwrap().push(RecordedRequest {
                meta: context.meta,
                url,
                headers: parts.headers,
                body: recorded_body,
                timeout: context.timeout,
            });

            // pop reply
            let reply = {
                let mut g = st.replies.lock().unwrap();
                g.pop_front().unwrap_or_else(|| {
                    let last = st.recorded.lock().unwrap().last().cloned();
                    panic!(
                        "MockTransport: no more scripted replies, but send() was called.\nlast_request={:#?}",
                        last
                    );
                })
            };

            let mut response =
                http::Response::new(concord_core::advanced::DynBody::from_bytes(reply.body));
            *response.status_mut() = reply.status;
            *response.headers_mut() = reply.headers;
            Ok(response)
        })
    }
}
