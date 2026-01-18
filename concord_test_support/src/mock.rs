use bytes::Bytes;
use concord_core::transport::*;
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub meta: RequestMeta,
    pub url: url::Url,
    pub headers: http::HeaderMap,
    pub body: Option<Bytes>,
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
}

struct OneShotBody {
    chunk: Option<Bytes>,
}

impl TransportBody for OneShotBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.chunk.take()) })
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

impl MockBuilder {
    pub fn new() -> Self {
        Self { replies: Vec::new() }
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

impl concord_core::prelude::Transport for MockTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let st = self.st.clone();
        Box::pin(async move {
            // record
            st.recorded.lock().unwrap().push(RecordedRequest {
                meta: req.meta.clone(),
                url: req.url.clone(),
                headers: req.headers.clone(),
                body: req.body.clone(),
                timeout: req.timeout,
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

            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: reply.status,
                headers: reply.headers,
                content_length: Some(reply.body.len() as u64),
                body: Box::new(OneShotBody {
                    chunk: Some(reply.body),
                }),
            })
        })
    }
}
