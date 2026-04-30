use bytes::Bytes;
use concord_core::advanced::{
    BuiltRequest, RateLimitPlan, Transport, TransportBody, TransportError, TransportResponse,
};
use concord_core::prelude::*;
use concord_macros::api;
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::future::{Future, ready};
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
        .paginate()
        .max_pages(3)
        .max_items(10)
        .collect()
        .await
        .expect("pagination collect succeeds");

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_query(&requests[0], "filter", "ranked");
    assert_query(&requests[0], "start", "0");
    assert_query(&requests[0], "count", "2");
    assert_query(&requests[1], "filter", "ranked");
    assert_query(&requests[1], "start", "2");
    assert_query(&requests[1], "count", "2");
}

#[tokio::test]
async fn generated_pagination_for_each_page_and_max_items_work() {
    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"["x"]"#)]);
    let api = PaginationHelperApi::new_with_transport(transport);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_for_callback = seen.clone();

    api.list()
        .count(2)
        .paginate()
        .for_each_page(move |page| {
            let seen = seen_for_callback.clone();
            async move {
                seen.lock().await.extend(page.value);
                Ok(())
            }
        })
        .await
        .expect("for_each_page succeeds");
    assert_eq!(*seen.lock().await, vec!["x".to_string()]);

    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"["a","b"]"#)]);
    let api = PaginationHelperApi::new_with_transport(transport);
    let err = api
        .list()
        .count(2)
        .paginate()
        .max_items(1)
        .for_each_page(|_| ready(Ok(())))
        .await
        .expect_err("max_items should fail");
    assert!(err.to_string().contains("max_items"));
}

fn assert_query(request: &BuiltRequest, key: &str, expected: &str) {
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
    requests: Arc<Mutex<Vec<BuiltRequest>>>,
}

impl RecordingTransport {
    fn new(responses: Vec<ResponseFixture>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn requests(&self) -> Vec<BuiltRequest> {
        self.requests.lock().await.clone()
    }
}

impl Transport for RecordingTransport {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            requests.lock().await.push(req.clone());
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
}

struct StaticBody(Option<Bytes>);

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.take()) })
    }
}
