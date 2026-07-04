use bytes::Bytes;
use concord_core::advanced::{
    RateLimitPlan, Transport, TransportBody, TransportError, TransportRequest, TransportResponse,
};
use concord_macros::api;
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};

mod retry_helper_contract {
    #![allow(unused_imports)]
    use super::*;

    api! {
        client RetryHelperApi {
            base "https://example.com"

            retry read {
                max_attempts 2
                methods [GET]
                on [500]
            }
        }

        GET Fetch
            as fetch
            path ["retry"]
            retry read
            -> Json<String>
    }

    pub(super) use retry_helper_api::RetryHelperApi;
}

use retry_helper_contract::RetryHelperApi;

#[derive(Clone)]
struct RecordingTransport {
    requests: Arc<StdMutex<Vec<RecordedRequest>>>,
    responses: Arc<StdMutex<VecDeque<ResponseFixture>>>,
}

#[derive(Clone, Debug)]
struct RecordedRequest {
    meta: concord_core::transport::RequestMeta,
    url: url::Url,
}

#[derive(Clone)]
struct ResponseFixture {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl ResponseFixture {
    fn json(body: &'static str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from_static(body.as_bytes()),
        }
    }

    fn json_status(status: StatusCode, body: &'static str) -> Self {
        let mut fixture = Self::json(body);
        fixture.status = status;
        fixture
    }
}

impl RecordingTransport {
    fn new(responses: Vec<ResponseFixture>) -> Self {
        Self {
            requests: Arc::new(StdMutex::new(Vec::new())),
            responses: Arc::new(StdMutex::new(responses.into())),
        }
    }

    async fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let requests = self.requests.clone();
        let responses = self.responses.clone();
        Box::pin(async move {
            requests.lock().expect("requests lock").push(RecordedRequest {
                meta: req.meta.clone(),
                url: req.url.clone(),
            });
            let response = responses.lock().expect("responses lock").pop_front();
            let response = response.expect("expected retry response fixture");
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: Some(response.body.len() as u64),
                rate_limit: RateLimitPlan::default(),
                body: Box::new(StaticBody(Some(response.body))),
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
async fn generated_retry_retries_then_succeeds_and_preserves_metadata() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json_status(StatusCode::INTERNAL_SERVER_ERROR, r#""retry""#),
        ResponseFixture::json(r#""ok""#),
    ]);
    let sent = transport.clone();
    let api = RetryHelperApi::new_with_transport(transport);

    let value = api
        .fetch()
        .execute()
        .await
        .expect("retry request should eventually succeed");
    assert_eq!(value, "ok");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    assert_eq!(requests[0].meta.endpoint, requests[1].meta.endpoint);
    assert_eq!(requests[0].meta.method, requests[1].meta.method);
    assert_eq!(requests[0].url.path(), "/retry");
    assert_eq!(requests[1].url.path(), "/retry");
}
