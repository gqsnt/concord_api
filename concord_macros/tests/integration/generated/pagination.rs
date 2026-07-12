use bytes::Bytes;
use concord_core::advanced::{DynBody, RequestExecutionContext, Transport, TransportError};
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

    GET List(filter?: String, tags?: Vec<String>, start: u64 = 0, count: u64 = 2)
        as list
        path ["items"]
        query {
            filter
            tags
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
        .tags(vec!["first".to_string(), "second".to_string()])
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
    assert_query_values(&requests[0], "tags", &["first", "second"]);
    assert_query(&requests[0], "start", "0");
    assert_query(&requests[0], "count", "2");
    assert_query(&requests[1], "filter", "ranked");
    assert_query_values(&requests[1], "tags", &["first", "second"]);
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
}

impl std::fmt::Debug for RecordedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let body = match &self.body {
            RecordedBody::Empty => "<empty>".to_string(),
            RecordedBody::Bytes(bytes) => format!("<{} bytes>", bytes.len()),
        };
        f.debug_struct("RecordedRequest")
            .field("meta", &self.meta)
            .field("url", &"<redacted>")
            .field(
                "headers",
                &concord_core::advanced::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &body)
            .field("timeout", &self.timeout)
            .finish()
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
        req: http::Request<DynBody>,
    ) -> Pin<Box<dyn Future<Output = Result<http::Response<DynBody>, TransportError>> + Send>> {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        Box::pin(async move {
            use http_body_util::BodyExt as _;
            let (parts, body) = req.into_parts();
            let context = parts
                .extensions
                .get::<RequestExecutionContext>()
                .cloned()
                .expect("context");
            let url = parts.uri.to_string().parse().expect("URL");
            let bytes = body
                .collect()
                .await
                .map_err(TransportError::new)?
                .to_bytes();
            let body = if bytes.is_empty() {
                RecordedBody::Empty
            } else {
                RecordedBody::Bytes(bytes)
            };
            requests.lock().await.push(RecordedRequest {
                meta: context.meta,
                url,
                headers: parts.headers,
                body,
                timeout: context.timeout,
            });
            let response = responses.lock().await.pop_front().expect("test response");
            let mut result = http::Response::new(DynBody::from_bytes(response.body));
            *result.status_mut() = response.status;
            *result.headers_mut() = response.headers;
            Ok(result)
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
        fixture.status = status;
        fixture
    }
}

fn assert_query_values(request: &RecordedRequest, key: &str, expected: &[&str]) {
    let values: Vec<String> = request
        .url
        .query_pairs()
        .filter(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned())
        .collect();
    assert_eq!(values, expected);
}
