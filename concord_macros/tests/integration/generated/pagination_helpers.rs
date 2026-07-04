use bytes::Bytes;
use concord_core::advanced::{
    RateLimitPlan, Transport, TransportBody, TransportError, TransportRequest, TransportResponse,
};
use concord_core::error::ErrorCategory;
use concord_core::prelude::*;
use concord_macros::api;
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

use self::pagination_helper_api::PaginationHelperApi;

api! {
    client PaginationHelperApi {
        base "https://example.com"
    }

    GET List(filter?: String, start: u64 = 0, count: u64 = 2)
        as list
        path ["items"]
        query {
            filter
            start
            count
        }
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Json<Vec<String>>
}

#[tokio::test]
async fn generated_pagination_collect_preserves_query_setters_and_caps() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"["a","b"]"#),
        ResponseFixture::json(r#"[]"#),
    ]);
    let sent = transport.clone();
    let api = PaginationHelperApi::new_with_transport(transport);

    let items = api
        .list()
        .filter("ranked".to_string())
        .count(2)
        .paginate(PaginationTermination::hard_page_cap(3))
        .collect()
        .await
        .expect("pagination collect succeeds");

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[1].meta.page_index, 1);
    assert_query(&requests[0], "filter", "ranked");
    assert_query(&requests[0], "start", "0");
    assert_query(&requests[0], "count", "2");
    assert_query(&requests[1], "filter", "ranked");
    assert_query(&requests[1], "start", "2");
    assert_query(&requests[1], "count", "2");
}

#[tokio::test]
async fn generated_pagination_collect_and_max_items_work() {
    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"["x"]"#)]);
    let api = PaginationHelperApi::new_with_transport(transport);

    let items = api
        .list()
        .count(2)
        .paginate(PaginationTermination::hard_page_cap(3))
        .collect()
        .await
        .expect("collect succeeds");
    assert_eq!(items, vec!["x".to_string()]);

    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"["a","b"]"#)]);
    let api = PaginationHelperApi::new_with_transport(transport);
    let err = api
        .list()
        .count(2)
        .paginate(PaginationTermination::hard_item_cap(1))
        .collect()
        .await
        .expect_err("hard item cap should fail");
    assert!(err.to_string().contains("hard item cap"));
}

#[tokio::test]
async fn generated_pagination_later_page_failure_is_typed_and_redacted() {
    let sentinel = "SECRET_GENERATED_PAGINATION_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"["a","b"]"#),
        ResponseFixture::json_status(StatusCode::INTERNAL_SERVER_ERROR, sentinel),
    ]);
    let api = PaginationHelperApi::new_with_transport(transport.clone());

    let err = api
        .list()
        .count(2)
        .paginate(PaginationTermination::hard_page_cap(3))
        .collect()
        .await
        .expect_err("later page failure should be typed");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "List");
    assert_eq!(err.context().method, http::Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    let requests = transport.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[1].meta.page_index, 1);
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
}

fn assert_query(request: &RecordedRequest, key: &str, expected: &str) {
    let value = request
        .url
        .query_pairs()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned());
    assert_eq!(value.as_deref(), Some(expected), "query key `{key}`");
}

#[derive(Clone)]
struct RecordingTransport {
    responses: Arc<Mutex<VecDeque<ResponseFixture>>>,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

struct RecordedRequest {
    meta: concord_core::transport::RequestMeta,
    url: url::Url,
    headers: http::HeaderMap,
    body: RecordedBody,
    timeout: Option<std::time::Duration>,
}

#[derive(Clone, Debug)]
enum RecordedBody {
    Empty,
    Bytes(Bytes),
    Stream,
}

struct EmptyDebugStream;

impl futures_core::Stream for EmptyDebugStream {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

impl std::fmt::Debug for RecordedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = match &self.body {
            RecordedBody::Empty => concord_core::advanced::TransportRequestBody::Empty,
            RecordedBody::Bytes(body) => {
                concord_core::advanced::TransportRequestBody::from_bytes(body.clone())
            }
            RecordedBody::Stream => concord_core::advanced::TransportRequestBody::Stream(
                concord_core::advanced::TransportByteStream::new(EmptyDebugStream),
            ),
        };
        let temp = TransportRequest {
            meta: self.meta.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body,
            timeout: self.timeout,
            rate_limit: RateLimitPlan::default(),
            transport_auth: None,
            extensions: concord_core::auth::RequestExtensions::default(),
        };
        write!(f, "{temp:?}")
    }
}

impl RecordingTransport {
    fn new(responses: Vec<ResponseFixture>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn requests(&self) -> Vec<RecordedRequest> {
        let mut requests = self.requests.lock().await;
        std::mem::take(&mut *requests)
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            let body = match req.body {
                concord_core::advanced::TransportRequestBody::Empty => RecordedBody::Empty,
                concord_core::advanced::TransportRequestBody::Bytes(body) => {
                    RecordedBody::Bytes(body)
                }
                concord_core::advanced::TransportRequestBody::Stream(_) => RecordedBody::Stream,
            };
            requests.lock().await.push(RecordedRequest {
                meta: req.meta.clone(),
                url: req.url.clone(),
                headers: req.headers.clone(),
                body,
                timeout: req.timeout,
            });
            let response = responses.lock().await.pop_front().expect("test response");
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
        if let Self::Buffered { status: s, .. } = &mut fixture {
            *s = status;
        }
        fixture
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
