use bytes::Bytes;
use concord_core::prelude::*;
use concord_core::transport::*;
use http::{HeaderMap, StatusCode};
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

pub fn json_bytes<T: Serialize>(v: &T) -> Bytes {
    Bytes::from(serde_json::to_vec(v).expect("json encode"))
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub url: url::Url,
    pub headers: http::HeaderMap,
    pub body: Option<Bytes>,
    pub timeout: Option<std::time::Duration>,
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

#[derive(Clone)]
pub struct MockTransport {
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
    replies: Arc<Mutex<Vec<MockReply>>>,
}

impl MockTransport {
    pub fn new(replies: Vec<MockReply>) -> (Self, Arc<Mutex<Vec<RecordedRequest>>>) {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let replies = Arc::new(Mutex::new(replies));
        (
            Self {
                recorded: recorded.clone(),
                replies,
            },
            recorded,
        )
    }
}

impl concord_core::prelude::Transport for MockTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let recorded = self.recorded.clone();
        let replies = self.replies.clone();

        Box::pin(async move {
            recorded.lock().unwrap().push(RecordedRequest {
                url: req.url.clone(),
                headers: req.headers.clone(),
                body: req.body.clone(),
                timeout: req.timeout,
            });

            let reply = replies.lock().unwrap().remove(0);

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
